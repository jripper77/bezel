# Preparar um cartão SD

[English](../sd-card.md)

As telas com entrada de cartão leem cartões **FAT32** com tabela de partição
**MBR**. O Bezel nunca formata o cartão: prepare-o no computador e depois
coloque na tela.

Cartões novos de até 32 GB costumam vir prontos (MBR e FAT32). Cartões maiores
vêm em exFAT, que a tela não lê: formate como abaixo. O Bezel foi testado com um
cartão de 32 GB.

**Formatar apaga tudo o que está no cartão.** Confira duas vezes que você
escolheu o cartão e não outro disco.

## Linux, com o Discos do GNOME

1. Abra o **Discos** e selecione o cartão na lista à esquerda (confira o
   tamanho).
2. No menu no canto de cima, à direita, escolha **Formatar disco…**, selecione
   **Compatível com todos os sistemas e dispositivos (MBR/DOS)** e confirme.
3. Clique no **+** abaixo do espaço vazio para criar uma partição do cartão
   inteiro, do tipo **Para uso com todos os sistemas e dispositivos (FAT)**, e
   confirme.

## Linux, no terminal

Ache o cartão pelo tamanho e pelo nome (por exemplo `sdb`, ou `mmcblk0` num
leitor embutido):

```bash
lsblk -o NAME,SIZE,MODEL,TRAN
```

Depois, trocando `sdX` pelo cartão (desmonte antes se o sistema o montou):

```bash
sudo parted /dev/sdX --script mklabel msdos mkpart primary fat32 1MiB 100%
sudo mkfs.vfat -F 32 /dev/sdX1
```

Num cartão `mmcblk0`, a partição é `/dev/mmcblk0p1`.

## Windows

No Explorador de Arquivos, clique com o botão direito no cartão, escolha
**Formatar…**, selecione **FAT32** e clique em **Iniciar**. O Windows só oferece
FAT32 para cartões de até 32 GB; para um cartão maior, formate no Linux ou com
um programa de formatação FAT32.

## Na tela

1. Com a tela desconectada, coloque o cartão.
2. Conecte a tela de novo.
3. Confira se o Bezel o vê: **Tela → Armazenamento** mostra o **Cartão SD** com o
   tamanho dele, ou rode `bezel storage info`.

"Nenhum cartão SD na tela" quer dizer que a tela não achou um cartão que ela
consiga ler: confira se o cartão está em FAT32 com tabela de partição MBR.
