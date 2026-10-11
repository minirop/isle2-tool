# Isle2 tool

A tool that can compress & decompress files for LEGO Island 2 on the GBA. If the file has a palette, can be converted to and from GIF.

LZ77 is a reimplementation of Luna's [gba-lz77](https://github.com/lunasorcery/gba-lz77).

## Usage

### Decompress

```sh
$ isle2-tool decompress <input> <output>
```

If `<input>.pal` exists, it means it's an image and will therefore be converted to a GIF file.

### Compress

```sh
$ isle2-tool compress <input> <output>
```

If `<input>` ends with `.gif`, the palette will be extracted in a file name `<output>.pal`.
