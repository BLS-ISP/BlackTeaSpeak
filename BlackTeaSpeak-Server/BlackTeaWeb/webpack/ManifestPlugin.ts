import * as webpack from "webpack";
import * as path from "node:path";
import {Compilation, NormalModule} from "webpack";

interface Options {
    outputFileName?: string;
    context: string;
}

interface EntrypointAsset {
    name: string;
}

interface StatsEntrypoint {
    assets?: EntrypointAsset[];
}

class ManifestGenerator {
    private readonly options: Options;

    constructor(options: Options) {
        this.options = options || { context: __dirname };
    }

    apply(compiler: webpack.Compiler) {
        compiler.hooks.thisCompilation.tap("ManifestGenerator", compilation => {
            compilation.hooks.processAssets.tap({
                name: "ManifestGenerator",
                stage: Compilation.PROCESS_ASSETS_STAGE_REPORT
            }, () => this.emitAssets(compilation));
        });
    }

    private resolveAssetHash(compilation: webpack.Compilation, file: string) {
        const contentHash = compilation.getAsset(file)?.info?.contenthash;

        if(Array.isArray(contentHash)) {
            return contentHash[0] || "";
        }

        if(typeof contentHash === "string") {
            return contentHash;
        }

        return "";
    }

    private collectEntrypointFiles(compilation: webpack.Compilation, assets: EntrypointAsset[] | undefined) {
        const fileJs: Array<{ hash: string; file: string }> = [];
        const filesCss: Array<{ hash: string; file: string }> = [];

        for(const asset of assets || []) {
            const file = asset.name;
            const fileHash = this.resolveAssetHash(compilation, file);
            const extension = path.extname(file);

            switch (extension) {
                case ".js":
                    fileJs.push({
                        hash: fileHash,
                        file: file
                    });
                    break;

                case ".css":
                    filesCss.push({
                        hash: fileHash,
                        file: file
                    });
                    break;

                case ".wasm":
                    break;

                default:
                    break;
            }
        }

        return { fileJs, filesCss };
    }

    private resolveChunkModule(compilation: webpack.Compilation, module: webpack.Module) {
        const moduleId = compilation.chunkGraph.getModuleId(module);
        const identifier = module.identifier();
        if(typeof identifier === "string" && identifier.startsWith("svg-sprites/")) {
            /* custom svg sprite handler */
            return {
                id: moduleId,
                context: "svg-sprites",
                resource: identifier.substring("svg-sprites/".length)
            };
        }

        if(!module.context) {
            return undefined;
        }

        if(!module.type.startsWith("javascript/")) {
            return undefined;
        }

        if(!(module instanceof NormalModule)) {
            return undefined;
        }

        if(module.resource.includes("webpack-dev-server")) {
            /* Don't include dev server files */
            return undefined;
        }

        const moduleDirectory = path.dirname(module.resource);
        if(module.context !== moduleDirectory) {
            throw new Error("invalid context/resource relation (" + module.context + " <-> " + moduleDirectory + ")");
        }

        return {
            id: moduleId,
            context: path.relative(this.options.context, module.context).replace(/\\/g, "/"),
            resource: path.basename(module.resource)
        };
    }

    private collectChunkModules(compilation: webpack.Compilation, chunkGroup: webpack.ChunkGroup) {
        const modules = [];

        for(const chunk of chunkGroup.chunks) {
            if(!chunk.files.size) {
                continue;
            }

            for(const module of compilation.chunkGraph.getChunkModules(chunk)) {
                const resolvedModule = this.resolveChunkModule(compilation, module);
                if(resolvedModule) {
                    modules.push(resolvedModule);
                }
            }
        }

        return modules;
    }

    emitAssets(compilation: webpack.Compilation) {
        const stats = compilation.getStats().toJson({
            all: false,
            entrypoints: true,
        }) as { entrypoints?: Record<string, StatsEntrypoint> };
        const chunkData = {};

        for(const [entryPointName, entryPoint] of compilation.entrypoints) {
            const entryPointStats = stats.entrypoints?.[entryPointName];
            if(!entryPointStats) {
                throw new Error("Missing entrypoint assets in webpack stats for " + entryPointName);
            }

            const { fileJs, filesCss } = this.collectEntrypointFiles(compilation, entryPointStats.assets);
            const modules = this.collectChunkModules(compilation, entryPoint);

            chunkData[entryPointName] = {
                files: fileJs,
                css_files: filesCss,
                modules: modules
            };
        }

        const payload = JSON.stringify({
            version: 2,
            chunks: chunkData
        });

        const fileName = this.options.outputFileName || "manifest.json";
        compilation.emitAsset(fileName, new webpack.sources.RawSource(payload));
    }
}

export = ManifestGenerator;