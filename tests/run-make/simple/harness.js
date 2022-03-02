const { WASI } = require("wasi");
const fs = require("fs/promises");

const main = async () => {
  const wasi = new WASI({
    args: process.argv.slice(3)
  });
  const binary = await fs.readFile(process.argv[2]);
  const imports = {
    wasi_snapshot_preview1: wasi.wasiImport,
  };
  const { instance } = await WebAssembly.instantiate(binary.buffer, imports);
  wasi.start(instance);
}

main()
