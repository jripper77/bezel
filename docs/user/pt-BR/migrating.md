# Vindo do turing-smart-screen-python

[English](../migrating.md)

O Bezel controla as mesmas telas que o
[turing-smart-screen-python](https://github.com/mathoudebine/turing-smart-screen-python)
e lê os temas dele. Os dois não podem rodar ao mesmo tempo.

## 1. Pare o outro programa

Dois programas na mesma tela misturam os dados, então o Bezel recusa uma porta
que outro programa está usando e diz qual, por exemplo `/dev/ttyACM0 is in use
by python3 (PID 4242)`.

- **Aberto num terminal:** aperte Ctrl+C lá.
- **Rodando como serviço do systemd:** ache o serviço e desligue-o, para ele não
  voltar no próximo boot:

  ```bash
  systemctl list-units --all | grep -i -E 'turing|smart'
  systemctl --user disable --now <nome dele>.service         # serviço de usuário
  sudo systemctl disable --now <nome dele>.service           # serviço do sistema
  ```

- **Windows:** feche-o pelo ícone da bandeja e tire-o de **Gerenciador de
  Tarefas → Aplicativos de inicialização** (ou do Agendador de Tarefas, se você o
  colocou lá).

O app do fabricante (TURZX / Turing) também segura a tela: feche-o pelo ícone da
bandeja antes de usar o Bezel.

## 2. Traga os seus temas

No aplicativo: **Temas → Importar…** e escolha a pasta do tema (por exemplo
`res/themes/MeuTema` dentro do turing-smart-screen-python) ou o `theme.yaml`
dele. O tema importado abre na edição; clique em **Salvar** para guardá-lo na
sua biblioteca. O que não deu para converter exatamente (um sensor que o Bezel
não tem, uma fonte que ele não acha) aparece numa lista depois da importação.

Pelo terminal:

```bash
bezel import turing-smart-screen-python/res/themes/MeuTema -o meutema.bezeltheme
bezel run meutema.bezeltheme
```

O `bezel run` e o `bezel render` também aceitam a pasta do tema direto e
convertem na hora. Os temas do app do fabricante (`.turtheme`) se importam do
mesmo jeito.

## 3. As suas configurações

| `config.yaml` | No Bezel |
|---|---|
| `COM_PORT` | achada sozinha; `--screen` escolhe uma quando há várias telas conectadas |
| `REVISION` | detectada pela tela |
| `DISPLAY_REVERSE` | **Girar 180°** no aplicativo; as orientações `-flipped` na linha de comando |
| `BRIGHTNESS` | **Tela → Ajustes → Brilho**, ou `bezel brightness 40` |
| `THEME` | o tema que você roda: [Iniciar com o computador](run-at-login.md) |
| `HW_SENSORS` | o Bezel lê os sensores sozinho; no Windows ele também usa o LibreHardwareMonitor ([Sensores](sensors.md)) |
| `PING` | **Preferências → Sensores**, ou `--ping-host` |

No Linux, se você rodava o programa com `sudo` ou tinha colocado o seu usuário no
grupo `dialout`, o Bezel não precisa de nenhum dos dois: veja
[Deixe o Bezel abrir a tela](permissions.md).
