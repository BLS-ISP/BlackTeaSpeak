const wabtFactory = require("wabt");
const filename = "module.wast";

export default function loader(this: any, source: string | Buffer): void {
    this.cacheable();
    const callback = this.async();

    Promise.resolve(wabtFactory())
        .then((wabt: any) => {
            const module = wabt.parseWat(filename, typeof source === "string" ? source : source.toString());
            const { buffer } = module.toBinary({ write_debug_names: false });

            callback(null, `module.exports = new Uint8Array([${Array.from(buffer).join(",")}]);`);
        })
        .catch((error: unknown) => callback(error as Error));
}