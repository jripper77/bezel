/*
 * LD_PRELOAD recorder for scripts/ci/studio-starts-silent.sh
 * (D-2026-10-01-gif-sticker-search-16, -17, -18).
 *
 * Every process of the tree that loads it appends one line per event to the
 * file named by $SILENT_LOG, tab-separated:
 *
 *   load  <pid> <ppid> <exe> <cmdline> <verdict>
 *   exec  <pid> <exe> <function> <program> <argv> <verdict>
 *   net   <pid> <exe> <function> <family> <address> <port> <scope> <verdict>
 *
 * `load` is written by every program image the shim enters (its constructor),
 * so a program started by any means that keeps the environment is recorded,
 * even by a call that does not go through the hooks below (glibc's own
 * system(), popen()). `exec` is written by execve, execv, execvp, execvpe,
 * execl, execlp, execle, fexecve, posix_spawn and posix_spawnp before the
 * real call; `net` by connect, and by sendto, sendmsg and sendmmsg when they
 * carry a destination. <scope> is `loopback` (127.0.0.0/8, ::1 and its
 * v4-mapped form), `outside` (any other AF_INET/AF_INET6 address) or `local`
 * (every other family: unix paths, `@name` for abstract ones, others by
 * number).
 *
 * With $SILENT_BLOCK=1 the shim also refuses what the script fails on, so a
 * regression never reaches the network or the desktop while it is tested:
 *   - a connection or datagram to an `outside` address, or to a loopback
 *     port other than 53 (the DNS stub), fails with ENETUNREACH;
 *   - one to a unix socket that is not in $SILENT_SOCKETS (entries separated
 *     by `:`; one ending in `/` names a folder of sockets, any other one a
 *     socket; abstract and unnamed addresses are never in it) fails with
 *     EACCES: the user's own session bus and display are never reached;
 *   - a program whose path is not in $SILENT_ALLOW (entries separated by
 *     `:`; one with a `/` names a file, one without names a basename) fails
 *     with EACCES, or exits with 126 at load when it was started some other
 *     way.
 * Other families (netlink...) are recorded and passed. <verdict> is `pass`
 * or `block`. Anything allowed goes to the real function (dlsym RTLD_NEXT)
 * unchanged; LD_PRELOAD is never stripped.
 *
 * Inside the hooks only async-signal-safe calls are made (open with
 * O_APPEND, write, close, readlink, getpid, getppid, string functions):
 * they run between fork and exec. The real functions are resolved in the
 * constructor; a hook reached before it resolves its own.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <spawn.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#define LINE_MAX_BYTES 4096
#define PATH_BYTES 4096
#define MAX_LIST_ARGS 1024

typedef int (*connect_fn)(int, const struct sockaddr *, socklen_t);
typedef ssize_t (*sendto_fn)(int, const void *, size_t, int, const struct sockaddr *, socklen_t);
typedef ssize_t (*sendmsg_fn)(int, const struct msghdr *, int);
typedef int (*sendmmsg_fn)(int, struct mmsghdr *, unsigned int, int);
typedef int (*execve_fn)(const char *, char *const[], char *const[]);
typedef int (*execv_fn)(const char *, char *const[]);
typedef int (*fexecve_fn)(int, char *const[], char *const[]);
typedef int (*spawn_fn)(pid_t *, const char *, const posix_spawn_file_actions_t *,
                        const posix_spawnattr_t *, char *const[], char *const[]);

static connect_fn real_connect;
static sendto_fn real_sendto;
static sendmsg_fn real_sendmsg;
static sendmmsg_fn real_sendmmsg;
static execve_fn real_execve;
static execv_fn real_execv;
static execv_fn real_execvp;
static execve_fn real_execvpe;
static fexecve_fn real_fexecve;
static spawn_fn real_posix_spawn;
static spawn_fn real_posix_spawnp;

static char log_path[PATH_BYTES];
static char allow[PATH_BYTES];
static char sockets[PATH_BYTES];
static char self_exe[PATH_BYTES];
static int blocking;
static int ready;

/* ---- a line built on the stack ---------------------------------------- */

struct line {
    char buf[LINE_MAX_BYTES];
    size_t len;
};

/* Keeps one byte for the line's newline ([`emit`]). */
static void put_raw(struct line *l, const char *s, size_t n) {
    if (n > sizeof l->buf - 1 - l->len) n = sizeof l->buf - 1 - l->len;
    memcpy(l->buf + l->len, s, n);
    l->len += n;
}

static void put_str(struct line *l, const char *s) { put_raw(l, s, strlen(s)); }

/* Text of unknown origin: `\`, tabs, spaces, newlines and non-ASCII bytes
 * as \xNN, so one field never splits. */
static void put_esc_n(struct line *l, const char *s, size_t n) {
    static const char hex[] = "0123456789abcdef";
    for (size_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)s[i];
        if (c > 0x20 && c < 0x7f && c != '\\') {
            put_raw(l, (const char *)&c, 1);
        } else {
            char e[4] = {'\\', 'x', hex[c >> 4], hex[c & 15]};
            put_raw(l, e, 4);
        }
    }
}

static void put_esc(struct line *l, const char *s) {
    if (s == NULL) {
        put_str(l, "(null)");
        return;
    }
    if (*s == '\0') {
        put_str(l, "(empty)");
        return;
    }
    put_esc_n(l, s, strlen(s));
}

static void put_dec(struct line *l, unsigned long v) {
    char d[24];
    size_t n = 0;
    do {
        d[sizeof d - 1 - n++] = (char)('0' + v % 10);
        v /= 10;
    } while (v != 0 && n < sizeof d);
    put_raw(l, d + sizeof d - n, n);
}

static void put_tab(struct line *l) { put_raw(l, "\t", 1); }

static void put_argv(struct line *l, char *const argv[]) {
    if (argv == NULL) {
        put_str(l, "(null)");
        return;
    }
    for (size_t i = 0; argv[i] != NULL && l->len < sizeof l->buf - 64; i++) {
        if (i > 0) put_raw(l, " ", 1);
        put_esc(l, argv[i]);
    }
}

static void read_exe(char *out, size_t size) {
    ssize_t n = readlink("/proc/self/exe", out, size - 1);
    out[n > 0 ? n : 0] = '\0';
}

/* ---- state ------------------------------------------------------------ */

static void copy_env(char *out, size_t size, const char *name) {
    const char *v = getenv(name);
    out[0] = '\0';
    if (v != NULL && strlen(v) < size) memcpy(out, v, strlen(v) + 1);
}

static void init_state(void) {
    if (ready) return;
    copy_env(log_path, sizeof log_path, "SILENT_LOG");
    copy_env(allow, sizeof allow, "SILENT_ALLOW");
    copy_env(sockets, sizeof sockets, "SILENT_SOCKETS");
    const char *b = getenv("SILENT_BLOCK");
    blocking = b != NULL && strcmp(b, "1") == 0;
    read_exe(self_exe, sizeof self_exe);
    real_connect = (connect_fn)dlsym(RTLD_NEXT, "connect");
    real_sendto = (sendto_fn)dlsym(RTLD_NEXT, "sendto");
    real_sendmsg = (sendmsg_fn)dlsym(RTLD_NEXT, "sendmsg");
    real_sendmmsg = (sendmmsg_fn)dlsym(RTLD_NEXT, "sendmmsg");
    real_execve = (execve_fn)dlsym(RTLD_NEXT, "execve");
    real_execv = (execv_fn)dlsym(RTLD_NEXT, "execv");
    real_execvp = (execv_fn)dlsym(RTLD_NEXT, "execvp");
    real_execvpe = (execve_fn)dlsym(RTLD_NEXT, "execvpe");
    real_fexecve = (fexecve_fn)dlsym(RTLD_NEXT, "fexecve");
    real_posix_spawn = (spawn_fn)dlsym(RTLD_NEXT, "posix_spawn");
    real_posix_spawnp = (spawn_fn)dlsym(RTLD_NEXT, "posix_spawnp");
    ready = 1;
}

static void emit(struct line *l) {
    if (log_path[0] == '\0') return;
    l->buf[l->len++] = '\n';
    int fd = open(log_path, O_WRONLY | O_APPEND | O_CREAT | O_CLOEXEC, 0600);
    if (fd < 0) return;
    ssize_t ignored = write(fd, l->buf, l->len);
    (void)ignored;
    close(fd);
}

static void start_line(struct line *l, const char *kind) {
    l->len = 0;
    put_str(l, kind);
    put_tab(l);
    put_dec(l, (unsigned long)getpid());
    put_tab(l);
    put_esc(l, self_exe);
}

/* ---- programs ---------------------------------------------------------- */

static const char *base_of(const char *path) {
    const char *slash = strrchr(path, '/');
    return slash == NULL ? path : slash + 1;
}

/* Whether `program` (a path, or a bare name looked up in PATH) is in
 * $SILENT_ALLOW. A bare name only matches basename entries. */
static int allowed(const char *program) {
    if (program == NULL) return 0;
    const char *base = base_of(program);
    const char *p = allow;
    while (*p != '\0') {
        const char *end = strchr(p, ':');
        size_t n = end == NULL ? strlen(p) : (size_t)(end - p);
        int names_file = memchr(p, '/', n) != NULL;
        const char *against = names_file ? program : base;
        if (n > 0 && strlen(against) == n && memcmp(p, against, n) == 0) return 1;
        if (end == NULL) break;
        p = end + 1;
    }
    return 0;
}

/* Logs a program start; answers whether to refuse it. */
static int record_exec(const char *function, const char *program, char *const argv[]) {
    init_state();
    int saved = errno;
    int block = blocking && !allowed(program);
    struct line l;
    start_line(&l, "exec");
    put_tab(&l);
    put_str(&l, function);
    put_tab(&l);
    put_esc(&l, program);
    put_tab(&l);
    put_argv(&l, argv);
    put_tab(&l);
    put_str(&l, block ? "block" : "pass");
    emit(&l);
    errno = saved;
    return block;
}

int execve(const char *path, char *const argv[], char *const envp[]) {
    if (record_exec("execve", path, argv)) {
        errno = EACCES;
        return -1;
    }
    return real_execve(path, argv, envp);
}

int execv(const char *path, char *const argv[]) {
    if (record_exec("execv", path, argv)) {
        errno = EACCES;
        return -1;
    }
    return real_execv(path, argv);
}

int execvp(const char *file, char *const argv[]) {
    if (record_exec("execvp", file, argv)) {
        errno = EACCES;
        return -1;
    }
    return real_execvp(file, argv);
}

int execvpe(const char *file, char *const argv[], char *const envp[]) {
    if (record_exec("execvpe", file, argv)) {
        errno = EACCES;
        return -1;
    }
    return real_execvpe(file, argv, envp);
}

int fexecve(int fd, char *const argv[], char *const envp[]) {
    struct line proc;
    proc.len = 0;
    put_str(&proc, "/proc/self/fd/");
    put_dec(&proc, (unsigned long)(fd < 0 ? 0 : fd));
    proc.buf[proc.len] = '\0';
    char path[PATH_BYTES];
    ssize_t n = readlink(proc.buf, path, sizeof path - 1);
    path[n > 0 ? n : 0] = '\0';
    if (record_exec("fexecve", path, argv)) {
        errno = EACCES;
        return -1;
    }
    return real_fexecve(fd, argv, envp);
}

/* execl, execlp, execle: the list as an array on the stack, then the
 * vector form above (which records it). */
#define COLLECT_ARGS(first, ap, argv, count)                     \
    do {                                                         \
        argv[0] = (char *)(first);                               \
        count = 1;                                               \
        while (count < MAX_LIST_ARGS) {                          \
            argv[count] = va_arg(ap, char *);                    \
            if (argv[count] == NULL) break;                      \
            count++;                                             \
        }                                                        \
        argv[count < MAX_LIST_ARGS ? count : MAX_LIST_ARGS] = 0; \
    } while (0)

int execl(const char *path, const char *arg, ...) {
    char *argv[MAX_LIST_ARGS + 1];
    size_t count;
    va_list ap;
    va_start(ap, arg);
    COLLECT_ARGS(arg, ap, argv, count);
    va_end(ap);
    return execv(path, argv);
}

int execlp(const char *file, const char *arg, ...) {
    char *argv[MAX_LIST_ARGS + 1];
    size_t count;
    va_list ap;
    va_start(ap, arg);
    COLLECT_ARGS(arg, ap, argv, count);
    va_end(ap);
    return execvp(file, argv);
}

int execle(const char *path, const char *arg, ...) {
    char *argv[MAX_LIST_ARGS + 1];
    size_t count;
    va_list ap;
    va_start(ap, arg);
    COLLECT_ARGS(arg, ap, argv, count);
    char *const *envp = va_arg(ap, char *const *);
    va_end(ap);
    return execve(path, argv, envp);
}

int posix_spawn(pid_t *pid, const char *path, const posix_spawn_file_actions_t *actions,
                const posix_spawnattr_t *attr, char *const argv[], char *const envp[]) {
    if (record_exec("posix_spawn", path, argv)) return EACCES;
    return real_posix_spawn(pid, path, actions, attr, argv, envp);
}

int posix_spawnp(pid_t *pid, const char *file, const posix_spawn_file_actions_t *actions,
                 const posix_spawnattr_t *attr, char *const argv[], char *const envp[]) {
    if (record_exec("posix_spawnp", file, argv)) return EACCES;
    return real_posix_spawnp(pid, file, actions, attr, argv, envp);
}

/* ---- connections ------------------------------------------------------- */

static void put_v4(struct line *l, const unsigned char *a) {
    for (int i = 0; i < 4; i++) {
        if (i > 0) put_raw(l, ".", 1);
        put_dec(l, a[i]);
    }
}

static void put_group(struct line *l, unsigned v) {
    static const char hex[] = "0123456789abcdef";
    char h[4];
    int n = 0;
    for (int s = 12; s >= 0; s -= 4) {
        unsigned d = (v >> s) & 15;
        if (d != 0 || n > 0 || s == 0) h[n++] = hex[d];
    }
    put_raw(l, h, (size_t)n);
}

/* RFC 5952 text: the longest run of two or more zero groups becomes `::`. */
static void put_v6(struct line *l, const unsigned char *a) {
    unsigned g[8];
    int best = -1, best_len = 0;
    for (int i = 0; i < 8; i++) g[i] = (unsigned)a[2 * i] << 8 | a[2 * i + 1];
    for (int i = 0; i < 8;) {
        int j = i;
        while (j < 8 && g[j] == 0) j++;
        if (j - i > best_len && j - i >= 2) {
            best = i;
            best_len = j - i;
        }
        i = j > i ? j : i + 1;
    }
    for (int i = 0; i < 8; i++) {
        if (i == best) {
            put_raw(l, "::", 2);
            i += best_len - 1;
            continue;
        }
        if (i > 0 && i != best + best_len) put_raw(l, ":", 1);
        put_group(l, g[i]);
    }
}

static int v6_is_loopback(const unsigned char *a) {
    static const unsigned char one[16] = {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1};
    static const unsigned char mapped[12] = {0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff};
    if (memcmp(a, one, 16) == 0) return 1;
    return memcmp(a, mapped, 12) == 0 && a[12] == 127;
}

/* Whether the unix socket `path` (`n` bytes, not terminated) is in
 * $SILENT_SOCKETS: equal to an entry, or directly inside (no further `/`)
 * an entry ending in `/`. */
static int socket_accepted(const char *path, size_t n) {
    const char *p = sockets;
    if (n == 0 || path[0] == '\0') return 0;
    while (*p != '\0') {
        const char *end = strchr(p, ':');
        size_t len = end == NULL ? strlen(p) : (size_t)(end - p);
        int folder = len > 0 && p[len - 1] == '/';
        int fits = folder ? n > len && memchr(path + len, '/', n - len) == NULL : n == len;
        if (len > 0 && fits && memcmp(p, path, len) == 0) return 1;
        if (end == NULL) break;
        p = end + 1;
    }
    return 0;
}

/* Logs a destination; answers 0 to let it through, or the errno that
 * refuses it (blocking: an outside address or a loopback port but 53 with
 * ENETUNREACH, a unix socket not in $SILENT_SOCKETS with EACCES). */
static int record_net(const char *function, const struct sockaddr *addr, socklen_t len) {
    init_state();
    int saved = errno;
    const char *scope = "local";
    int refuse = 0;
    int refusal = ENETUNREACH;
    struct line l;
    start_line(&l, "net");
    put_tab(&l);
    put_str(&l, function);
    put_tab(&l);
    if (addr == NULL || len < (socklen_t)sizeof(sa_family_t)) {
        put_str(&l, "none\t-\t-");
    } else if (addr->sa_family == AF_INET && len >= (socklen_t)sizeof(struct sockaddr_in)) {
        const struct sockaddr_in *in = (const struct sockaddr_in *)addr;
        const unsigned char *a = (const unsigned char *)&in->sin_addr;
        put_str(&l, "inet\t");
        put_v4(&l, a);
        put_tab(&l);
        put_dec(&l, ntohs(in->sin_port));
        scope = a[0] == 127 ? "loopback" : "outside";
        refuse = a[0] != 127 || ntohs(in->sin_port) != 53;
    } else if (addr->sa_family == AF_INET6 && len >= (socklen_t)sizeof(struct sockaddr_in6)) {
        const struct sockaddr_in6 *in6 = (const struct sockaddr_in6 *)addr;
        const unsigned char *a = (const unsigned char *)&in6->sin6_addr;
        put_str(&l, "inet6\t");
        put_v6(&l, a);
        put_tab(&l);
        put_dec(&l, ntohs(in6->sin6_port));
        scope = v6_is_loopback(a) ? "loopback" : "outside";
        refuse = !v6_is_loopback(a) || ntohs(in6->sin6_port) != 53;
    } else if (addr->sa_family == AF_INET || addr->sa_family == AF_INET6) {
        put_str(&l, addr->sa_family == AF_INET ? "inet\t(short)\t-" : "inet6\t(short)\t-");
        scope = "outside";
        refuse = 1;
    } else if (addr->sa_family == AF_UNIX) {
        const struct sockaddr_un *un = (const struct sockaddr_un *)addr;
        size_t off = offsetof(struct sockaddr_un, sun_path);
        size_t n = len > off ? (size_t)len - off : 0;
        if (n > sizeof un->sun_path) n = sizeof un->sun_path;
        put_str(&l, "unix\t");
        refuse = 1;
        refusal = EACCES;
        if (n == 0) {
            put_str(&l, "(unnamed)");
        } else if (un->sun_path[0] == '\0') {
            put_raw(&l, "@", 1);
            put_esc_n(&l, un->sun_path + 1, n - 1);
        } else {
            size_t path_len = strnlen(un->sun_path, n);
            put_esc_n(&l, un->sun_path, path_len);
            refuse = !socket_accepted(un->sun_path, path_len);
        }
        put_str(&l, "\t-");
    } else {
        put_str(&l, "family-");
        put_dec(&l, addr->sa_family);
        put_str(&l, "\t-\t-");
    }
    int block = blocking && refuse;
    put_tab(&l);
    put_str(&l, scope);
    put_tab(&l);
    put_str(&l, block ? "block" : "pass");
    emit(&l);
    errno = saved;
    return block ? refusal : 0;
}

int connect(int fd, const struct sockaddr *addr, socklen_t len) {
    int refused = record_net("connect", addr, len);
    if (refused) {
        errno = refused;
        return -1;
    }
    if (real_connect == NULL) init_state();
    return real_connect(fd, addr, len);
}

ssize_t sendto(int fd, const void *buf, size_t n, int flags, const struct sockaddr *addr,
               socklen_t len) {
    int refused = addr != NULL ? record_net("sendto", addr, len) : 0;
    if (refused) {
        errno = refused;
        return -1;
    }
    if (real_sendto == NULL) init_state();
    return real_sendto(fd, buf, n, flags, addr, len);
}

ssize_t sendmsg(int fd, const struct msghdr *msg, int flags) {
    int refused = msg != NULL && msg->msg_name != NULL && msg->msg_namelen > 0
                      ? record_net("sendmsg", (const struct sockaddr *)msg->msg_name,
                                   msg->msg_namelen)
                      : 0;
    if (refused) {
        errno = refused;
        return -1;
    }
    if (real_sendmsg == NULL) init_state();
    return real_sendmsg(fd, msg, flags);
}

int sendmmsg(int fd, struct mmsghdr *vec, unsigned int count, int flags) {
    int refused = 0;
    for (unsigned int i = 0; vec != NULL && i < count; i++) {
        const struct msghdr *m = &vec[i].msg_hdr;
        if (m->msg_name == NULL || m->msg_namelen == 0) continue;
        int one = record_net("sendmmsg", (const struct sockaddr *)m->msg_name, m->msg_namelen);
        if (refused == 0) refused = one;
    }
    if (refused) {
        errno = refused;
        return -1;
    }
    if (real_sendmmsg == NULL) init_state();
    return real_sendmmsg(fd, vec, count, flags);
}

/* ---- every program image the shim enters ------------------------------- */

__attribute__((constructor)) static void silent_load(void) {
    init_state();
    int block = blocking && !allowed(self_exe);
    struct line l;
    l.len = 0;
    put_str(&l, "load\t");
    put_dec(&l, (unsigned long)getpid());
    put_tab(&l);
    put_dec(&l, (unsigned long)getppid());
    put_tab(&l);
    put_esc(&l, self_exe);
    put_tab(&l);
    int fd = open("/proc/self/cmdline", O_RDONLY | O_CLOEXEC);
    if (fd >= 0) {
        char cmd[1024];
        ssize_t n = read(fd, cmd, sizeof cmd);
        close(fd);
        for (ssize_t i = 0; i + 1 < n; i++)
            if (cmd[i] == '\0') cmd[i] = ' ';
        /* Spaces between arguments survive; the rest is escaped. */
        for (ssize_t i = 0; i < n && cmd[i] != '\0'; i++) {
            if (cmd[i] == ' ') put_raw(&l, " ", 1);
            else put_esc_n(&l, cmd + i, 1);
        }
    }
    put_tab(&l);
    put_str(&l, block ? "block" : "pass");
    emit(&l);
    if (block) _exit(126);
}
