# GIFs e stickers

[English](../gifs-and-stickers.md)

O Bezel Studio busca GIFs e stickers no KLIPY, uma biblioteca gratuita de
GIFs, guarda os que você escolher na sua **coleção**, neste computador, e os usa
em qualquer tema, como as imagens que você mesmo adiciona. Um sticker tem fundo
transparente: o tema aparece através dele.

O Bezel não tem uma chave do KLIPY própria: você obtém uma de graça em poucos
minutos e cola uma vez. As suas buscas usam a sua chave e contam no limite
dela. Sem chave, o Bezel nunca se conecta ao KLIPY.

## 1. Obtenha a sua chave do KLIPY

1. Abra o Painel de Parceiros do KLIPY (*Partner Panel*),
   [partner.klipy.com](https://partner.klipy.com), e crie uma conta gratuita.
2. Crie um app lá e copie a chave de API (*API key*) dele.
3. Cole a chave no Bezel e salve
   ([próximo passo](#2-salve-a-chave-no-bezel)).
4. Uma chave nova é uma **chave de teste**: ela permite **100 requisições por
   hora**, o bastante para buscar de vez em quando. Para mais, peça ao KLIPY o
   acesso de produção ([Limites](#limites)).

O botão **?**, ao lado do campo da chave no Bezel, mostra esses mesmos passos,
com **Abrir o Painel de Parceiros** e **Abrir o guia** (esta página). Esc, ou um
clique fora, fecha a ajuda.

## 2. Salve a chave no Bezel

1. Abra a aba **Mídia** e depois **Coleção** (ao lado de **Deste tema**).
2. Clique em **Buscar GIFs e stickers…**. A janela abre com o campo da chave no
   topo.
3. Cole a chave em **Chave da API do KLIPY** e clique em **Salvar**.

Salvar não envia nada ao KLIPY: uma chave que o KLIPY recusa aparece na sua
primeira busca, que então abre a ajuda. Depois de salva, o Bezel mostra só os 4
últimos caracteres da chave (**Chave salva, termina em** seguidos deles), nunca
mais a chave inteira. **Remover** apaga a chave deste computador.

## 3. Busque

Em **Buscar GIFs e stickers**:

- Escolha **GIFs** ou **Stickers**.
- Digite no campo de busca (o texto de exemplo dele é **Search KLIPY**, em
  inglês, como o KLIPY exige). O Bezel busca quando você para de digitar, com 2
  caracteres ou mais; Enter busca na hora. **Em alta** mostra o que está
  popular agora, sem digitar nada.
- Os resultados vêm de 24 em 24; **Carregar mais** traz os próximos 24.
  **Powered by KLIPY**, embaixo dos resultados, dá o crédito de onde eles vêm.
- Nada é buscado quando a janela abre. Uma página de resultados já mostrada
  nesta sessão não é pedida de novo, então voltar a ela não custa nada.
- Com o computador configurado para reduzir movimento, as prévias são imagens
  paradas.

Pelo teclado: as setas andam pelos resultados, Home e End vão ao primeiro e ao
último, Enter adiciona o selecionado à coleção. Leitores de tela anunciam
quantos resultados vieram, os erros e o que foi adicionado.

### Resultados explícitos

**Mostrar resultados explícitos** começa desligado toda vez que o Bezel abre.
Desligado, o Bezel pede ao KLIPY resultados com classificação G e PG (livre e
com orientação dos pais; o filtro de conteúdo `medium` do KLIPY): seguro numa
tela em cima da mesa, e os GIFs de reação do dia a dia continuam. Ligado, o
Bezel pede resultados sem filtro (o filtro `off` do KLIPY), que podem incluir
conteúdo adulto. A escolha vale até o Bezel fechar, e mudá-la busca de novo a
partir da primeira página. Ajustes de conteúdo que você fizer para a sua chave
no Painel de Parceiros podem restringir mais os resultados.

## 4. Adicione à coleção

Clique em **Adicionar à coleção** num resultado (ou aperte Enter nele). O Bezel
baixa a maior versão em GIF dele que tenha no máximo **25 MiB**, o máximo que
uma tela Turing rev C aceita por arquivo, para o GIF poder ir também para a tela
do jeito que está
([Qual o tamanho máximo de um arquivo](storage-and-video.md#qual-o-tamanho-máximo-de-um-arquivo)).

- Adicionar o mesmo GIF duas vezes guarda uma cópia só: o Bezel o reconhece
  pelo conteúdo.
- Se o que chegar não for um GIF, nada é guardado e o Bezel avisa. Um item sem
  versão em GIF de até 25 MiB não pode ser adicionado.
- O item ganha o título do resultado como nome, que você pode mudar.

## 5. Use e organize a coleção

**Mídia → Coleção** mostra o que você adicionou, com um filtro (**Todos**,
**GIFs**, **Stickers**), quantos itens há e o tamanho total deles. Para cada
item:

- **Adicionar como imagem**: um elemento de imagem no centro da área de edição.
  Um GIF animado se mexe no ritmo dele ([GIFs animados](first-theme.md#gifs-animados)).
- **Usar como fundo**: o GIF vira o fundo do tema, como um GIF animado
  adicionado por **Adicionar vídeo…**: ele ganha uma imagem de capa (com o
  ffmpeg) e a tela o toca como vídeo
  ([Um vídeo no fundo](first-theme.md#um-vídeo-no-fundo)).
- **Arraste** o item para a área de edição: um elemento de imagem onde você o
  soltar.
- **Renomear**: edite o nome ali mesmo; ele não pode ficar vazio.
- **Excluir…**: pergunta antes, nomeando os seus temas (e o tema aberto agora)
  que usam o mesmo GIF. Eles guardam a própria cópia e continuam funcionando.
  Excluir não pode ser desfeito.

Usar um item copia o GIF para dentro do tema, com o nome do item, então o tema
(e o arquivo `.bezeltheme` dele) leva o GIF junto e o item aparece em **Deste
tema**. As partes transparentes de um sticker mostram o que está por baixo, o
fundo ou outros elementos, em temas verticais e horizontais. Com movimento
reduzido, a coleção mostra imagens paradas.

## Limites

- **100 requisições por hora** com uma chave de teste. Cada página de
  resultados (uma busca, **Em alta**, **Carregar mais**) é uma requisição à API
  do KLIPY; uma página já mostrada na sessão não é pedida de novo, e o Bezel
  espera uma pausa na digitação em vez de buscar a cada letra.
- **Produção**: quando a chave de teste não bastar, no Painel de Parceiros ache
  a sua chave, abra o menu de três pontos dela e escolha **Request
  Production**, depois preencha o formulário curto. O KLIPY decide o pedido; a
  mesma chave continua funcionando no Bezel, sem mudar nada.
- **25 MiB** por GIF adicionado à coleção (veja
  [Adicione à coleção](#4-adicione-à-coleção)).
- Só GIF: as versões WebP e MP4 do KLIPY não são usadas. A linha de comando não
  busca no KLIPY.

## Quando algo dá errado

### A chave é recusada

O KLIPY respondeu que a chave não vale: o Bezel abre a ajuda da chave. Confira
se você copiou a chave inteira e nada mais, e se ela ainda existe no Painel de
Parceiros; cole de novo e clique em **Salvar**. O Bezel aceita só letras,
dígitos, `_` e `-` (até 128 caracteres) e avisa quando o campo tem outra coisa.

### "A chave chegou ao limite"

A chave fez as 100 requisições desta hora (numa chave de teste). Espere a hora
passar, ou peça o acesso de produção ([Limites](#limites)): a mensagem tem um
botão que abre o Painel de Parceiros. Os resultados já mostrados continuam, e a
coleção continua funcionando.

### O KLIPY não responde

Sem conexão, ou quando o KLIPY não responde em 10 segundos, a busca diz que o
KLIPY está indisponível. Confira a conexão; um firewall ou proxy precisa deixar
o Bezel chegar a `api.klipy.com` e `static.klipy.com` por HTTPS. A coleção fica
neste computador e continua funcionando sem internet, assim como os temas que
usam os itens dela.

### O campo de busca está desativado

Ainda não há chave: salve uma antes ([Salve a chave no Bezel](#2-salve-a-chave-no-bezel)).

## Os seus dados

### Privacidade

**Quando.** Nada chega ao KLIPY quando o Bezel abre, nem nunca sem uma chave.
Salvar a chave não envia nada. O Bezel só se conecta quando você age: uma busca
(ou **Em alta**), **Carregar mais**, as prévias dos resultados na tela e
**Adicionar à coleção**.

**O quê, e para quem.** Tudo vai para o KLIPY, por HTTPS, e para mais ninguém:

- Para `api.klipy.com`, a cada página de resultados: a sua chave (dentro do
  endereço da requisição), o texto que você digitou (nada em **Em alta**), GIFs
  ou stickers, o número da página e 24 resultados por página, o filtro de
  conteúdo escolhido por **Mostrar resultados explícitos**, os formatos que o
  Bezel quer (GIF, e imagens paradas em JPEG), `BR` como região quando o Bezel
  está em português, e um identificador de cliente: um número aleatório que o
  Bezel criou quando você salvou a chave, que não diz nada sobre você. Essas
  requisições nunca seguem um redirecionamento, então a chave não vai para
  nenhum outro servidor.
- Para `static.klipy.com`, sem a sua chave: as prévias dos resultados
  mostrados e o GIF que você adiciona à coleção.
- Como qualquer servidor da web, o KLIPY vê o seu endereço IP e quando cada
  requisição chega; a política de privacidade dele diz o que ele faz com isso.

Nada mais é enviado: sem anúncios, sem estatísticas de uso, e o Bezel não usa
os recursos de compartilhar ou denunciar do KLIPY.

**Onde fica a sua chave.** Só neste computador, em `klipy.json` na pasta de
configuração do Bezel, ao lado das preferências:

- Linux: `~/.config/io.github.slipalison.bezel/klipy.json` (ou dentro de
  `$XDG_CONFIG_HOME`);
- Windows: `%APPDATA%\io.github.slipalison.bezel\klipy.json`.

Só o seu usuário consegue ler o arquivo (no Linux, as permissões dele são
`0600`). A chave fica nele em texto simples, não no chaveiro do sistema. A
janela do Bezel nunca a recebe de volta (só os 4 últimos caracteres), e ela
nunca aparece nas mensagens nem nos registros (logs) do Bezel.

**Onde fica a sua coleção.** Na sua pasta de dados, ao lado das
[cópias locais](storage-and-video.md#as-cópias-locais-do-bezel) do Bezel:

- Linux: `~/.local/share/bezel/collection` (ou `$XDG_DATA_HOME/bezel/collection`);
- Windows: `%APPDATA%\bezel\collection`.

`collection.json` lista o nome de cada item, o tipo, o tamanho, quando você o
adicionou e de onde ele veio (o id do KLIPY e o endereço da página); `files/`
guarda os GIFs e `previews/` as prévias. Um tema que usa um item tem a própria
cópia dentro do tema.

**Guardar os downloads.** O Bezel guarda o que você adiciona até você excluir.
Os termos da API do KLIPY dizem se, e por quanto tempo, GIFs baixados podem ser
guardados: confira-os para a sua chave e exclua os itens que eles não deixarem
guardar.

### Remover a chave e a coleção

- **Remover**, ao lado do campo da chave, apaga o `klipy.json`; a busca fica
  desativada até você salvar uma chave de novo, que ganha um novo identificador
  de cliente.
- Para revogar a própria chave, exclua-a no Painel de Parceiros.
- **Excluir…** tira um item da coleção. Para remover a coleção inteira, feche o
  Bezel e apague a pasta `collection` acima. Os temas guardam as próprias cópias
  nos dois casos.
