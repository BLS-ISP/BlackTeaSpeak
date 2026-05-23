import * as fs from "node:fs";
import * as util from "node:util";
import * as path from "node:path";
import * as child_process from "node:child_process";

import {GeneratedAssetPlugin} from "./webpack/GeneratedAssetPlugin";

import { DefinePlugin, Configuration, } from "webpack";

import { Plugin as SvgSpriteGenerator } from "webpack-svg-sprite-generator";
import ManifestGenerator from "./webpack/ManifestPlugin";
import HtmlWebpackInlineSourcePlugin from "./webpack/HtmlWebpackInlineSource";
import TranslateableWebpackPlugin from "./tools/trgen/webpack/Plugin";

import ZipWebpackPlugin from "zip-webpack-plugin";
import HtmlWebpackPlugin from "html-webpack-plugin";
import MiniCssExtractPlugin from "mini-css-extract-plugin";
import CssMinimizerPlugin from "css-minimizer-webpack-plugin";
import TerserPlugin from "terser-webpack-plugin";
import CopyWebpackPlugin from "copy-webpack-plugin";

const { CleanWebpackPlugin } = require("clean-webpack-plugin");
const { BundleAnalyzerPlugin } = require("webpack-bundle-analyzer");

let developmentMode = false;

const resolveWebpackMode = (env: any, argv?: { mode?: string }): string => {
    if(argv && typeof argv.mode === "string") {
        return argv.mode;
    }

    if(env && typeof env.mode === "string") {
        return env.mode;
    }

    if(env?.WEBPACK_SERVE !== undefined) {
        return "development";
    }

    if(typeof process.env.NODE_ENV === "string") {
        return process.env.NODE_ENV;
    }

    return "production";
};

interface LocalBuildInfo {
    target: "client" | "web",
    mode: "debug" | "release",

    gitVersion: string,
    gitTimestamp: number,

    unixTimestamp: number,
    localTimestamp: string
}

let localBuildInfo: LocalBuildInfo;

const readCompatBuildVersion = (): string => {
    try {
        const packageInfo = JSON.parse(fs.readFileSync(path.join(__dirname, "package.json")).toString());
        if(typeof packageInfo.version === "string" && packageInfo.version.length > 0) {
            return packageInfo.version;
        }
    } catch (error) {
        console.warn("Failed to read package version fallback: %o", error);
    }

    return "compat";
};

const generateLocalBuildInfo = async (target: string): Promise<LocalBuildInfo> => {
    let info: LocalBuildInfo = {} as any;

    info.target = target as any;
    info.mode = developmentMode ? "debug" : "release";

    try {
        const gitHeadPath = path.join(__dirname, ".git", "HEAD");
        const gitRevision = fs.readFileSync(gitHeadPath).toString();
        if(gitRevision.includes("/")) {
            info.gitVersion = fs.readFileSync(path.join(__dirname, ".git", gitRevision.slice(5).trim())).toString().slice(0, 8);
        } else {
            info.gitVersion = (gitRevision || "00000000").slice(0, 8);
        }
    } catch (error) {
        console.warn("Falling back to compat build version because git metadata is unavailable: %o", error);
        info.gitVersion = readCompatBuildVersion();
    }

    try {
        const { stdout } = await util.promisify(child_process.exec)("git show -s --format=%ct", { cwd: __dirname });
        info.gitTimestamp = Number.parseInt(stdout.toString());
        if(Number.isNaN(info.gitTimestamp)) {
            throw new TypeError("failed to parse timestamp '" + stdout.toString() + "'");
        }
    } catch (error) {
        console.warn("Falling back to current timestamp because git history is unavailable: %o", error);
        info.gitTimestamp = Math.floor(Date.now() / 1000);
    }

    info.unixTimestamp = Date.now();
    info.localTimestamp = new Date().toString();

    return info;
};

const generateDefinitions = async (target: string) => {
    return {
        "__build": {
            target: JSON.stringify(target),
            mode: JSON.stringify(developmentMode ? "debug" : "release"),
            version: JSON.stringify(localBuildInfo.gitVersion),
            timestamp: localBuildInfo.gitTimestamp,
            entry_chunk_name: JSON.stringify("main-app")
        } as BuildDefinitions
    } as any;
};

const generateIndexPlugin = (target: "web" | "client"): HtmlWebpackPlugin => {
    const options: HtmlWebpackPlugin.Options & { inlineSource?: RegExp | string } = {};

    options.cache = true;
    options.chunks = ["loader"];
    options.inject = false;
    options.template = path.join(__dirname, "loader", "index.ejs");
    options.templateParameters = { buildTarget: target };
    options.scriptLoading = "defer";

    if(!developmentMode) {
        options.minify = {
            html5: true,

            collapseWhitespace: true,
            removeComments: true,
            removeRedundantAttributes: true,
            removeScriptTypeAttributes: true,
            removeTagWhitespace: true,
            minifyCSS: true,
            minifyJS: true,
            minifyURLs: true,
        };

        options.inlineSource = /\.(js|css)$/;
    }
    return new HtmlWebpackPlugin(options);
}

export const config = async (env: any, target: "web" | "client", argv?: { mode?: string }): Promise<Configuration & { devServer: any }> => {
    const activeMode = resolveWebpackMode(env, argv);
    developmentMode = activeMode === "development";
    console.log("Webpacking for %s (%s)", developmentMode ? "development" : "production", activeMode);

    localBuildInfo = await generateLocalBuildInfo(target);

    const translateablePlugin = new TranslateableWebpackPlugin({ assetName: "translations.json" });

    return {
        entry: {
            "loader": "./loader/app/index.ts",
            "modal-external": "./shared/js/entry-points/ModalWindow.ts",
        },

        devtool: developmentMode ? "inline-source-map" : "source-map",
        mode: developmentMode ? "development" : "production",
        plugins: [
            new CleanWebpackPlugin(),

            new DefinePlugin(await generateDefinitions(target)),
            new GeneratedAssetPlugin({
                customFiles: [
                    {
                        assetName: "buildInfo.json",
                        content: JSON.stringify(localBuildInfo)
                    }
                ]
            }),
            new ManifestGenerator({
                outputFileName: "manifest.json",
                context: __dirname
            }),

            new CopyWebpackPlugin({
                patterns: [
                    {
                        from: path.join(__dirname, "shared", "img"),
                        to: 'img',
                        globOptions: {
                            ignore: [
                                '**/client-icons/**',
                                //'**/style/**',
                            ]
                        }
                    },
                    target === "web" ? { from: path.join(__dirname, "shared", "i18n"), to: 'i18n' } : undefined,
                    { from: path.join(__dirname, "shared", "audio"), to: 'audio' }
                ].filter(e => !!e)
            }),

            new MiniCssExtractPlugin({
                filename: developmentMode ? "css/[name].[contenthash].css" : "css/[contenthash].css",
                chunkFilename: developmentMode ? "css/[name].[contenthash].css" : "css/[contenthash].css",
                ignoreOrder: true,

            }),
            new SvgSpriteGenerator({
                dtsOutputFolder: path.join(__dirname, "shared", "svg-sprites"),
                publicPath: "/",
                configurations: {
                    "client-icons": {
                        folder: path.join(__dirname, "shared", "img", "client-icons"),
                        cssClassPrefix: "client-",
                        cssOptions: [
                            {
                                scale: 1,
                                selector: ".icon",
                                unit: "px"
                            },
                            {
                                scale: 1.5,
                                selector: ".icon_x24",
                                unit: "px"
                            },
                            {
                                scale: 2,
                                selector: ".icon_x32",
                                unit: "px"
                            },
                            {
                                scale: 1,
                                selector: ".icon_em",
                                unit: "em"
                            }
                        ],
                        dtsOptions: {
                            enumName: "ClientIcon",
                            classUnionName: "ClientIconClass",
                            module: false
                        }
                    },
                    "country-flags": {
                        folder: path.join(__dirname, "shared", "img", "country-flags"),
                        cssClassPrefix: "flag-",
                        cssOptions: [
                            {
                                scale: 1,
                                selector: ".flag_em",
                                unit: "em"
                            }
                        ],
                        dtsOptions: {
                            enumName: "CountryFlag",
                            classUnionName: "CountryFlagClass",
                            module: false
                        }
                    }
                }
            }),

            generateIndexPlugin(target),
            new HtmlWebpackInlineSourcePlugin(HtmlWebpackPlugin),

            translateablePlugin,
            //new BundleAnalyzerPlugin(),

            env.package ? new ZipWebpackPlugin({
                path: path.join(__dirname, "dist-package"),
                filename: `${target === "web" ? "BlackTeaWeb" : "TeaClient"}-${developmentMode ? "development" : "release"}-${localBuildInfo.gitVersion}.zip`,
            }) : undefined
        ].filter(e => !!e),

        module: {
            rules: [
                {
                    test: /node_modules[\\/](react-resizable|react-grid-layout)[\\/]css[\\/]styles\.css$/,
                    use: [
                        "style-loader",
                        {
                            loader: "css-loader",
                            options: {
                                modules: false,
                                url: false,
                            }
                        }
                    ]
                },
                {
                    test: /\.(s[ac]|c)ss$/,
                    exclude: /node_modules[\\/](react-resizable|react-grid-layout)[\\/]css[\\/]styles\.css$/,
                    use: [
                        //'style-loader',
                        {
                            loader: MiniCssExtractPlugin.loader,
                            options: {
                                esModule: false,
                            }
                        },
                        {
                            loader: 'css-loader',
                            options: {
                                modules: {
                                    mode: "local",
                                    localIdentName: developmentMode ? "[path][name]__[local]--[hash:base64:5]" : "[hash:base64]",
                                },
                                sourceMap: developmentMode
                            }
                        },
                        {
                            loader: "postcss-loader",
                            options: {
                                postcssOptions: {
                                    config: path.resolve(__dirname, "postcss.config.js")
                                }
                            }
                        },
                        {
                            loader: "sass-loader",
                            options: {
                                api: "modern",
                                implementation: require("sass"),
                                sourceMap: developmentMode
                            }
                        }
                    ]
                },
                {
                    test: /\.tsx?$/,
                    exclude: /node_modules/,

                    use: [
                        {
                            loader: "babel-loader",
                            options: {
                                configFile: path.resolve(__dirname, "babel.config.js"),
                                presets: ["@babel/preset-env"]
                            }
                        },
                        {
                            loader: "ts-loader",
                            options: {
                                context: __dirname,
                                colors: true,
                                getCustomTransformers: program => ({
                                    before: [ translateablePlugin.createTypeScriptTransformer(program) ]
                                }),
                                transpileOnly: developmentMode
                            }
                        }
                    ]
                },
                {
                    test: /\.was?t$/,
                    use: [
                        "./webpack/WatLoader.js"
                    ]
                },
                {
                    test: /\.html$/i,
                    use: [ translateablePlugin.createTemplateLoader() ],
                    type: "asset/source",
                },
                {
                    test: /ChangeLog\.md$/i,
                    type: "asset/source",
                },
                {
                    test: /\.svg$/,
                    use: [{
                        loader: '@svgr/webpack',
                        options: {
                            svgoConfig: {
                                plugins: [
                                    {
                                        name: "preset-default",
                                        params: {
                                            overrides: {
                                                removeViewBox: false
                                            }
                                        }
                                    }
                                ]
                            }
                        }
                    }],
                },
                {
                    test: /\.(png|jpg|jpeg|gif)?$/,
                    type: "asset/resource",
                    generator: {
                        filename: 'img/[hash][ext][query]'
                    }
                },
            ]
        } as any,
        resolve: {
            extensions: ['.tsx', '.ts', '.js', ".scss"],
            alias: {
                "vendor/xbbcode": path.resolve(__dirname, "vendor/xbbcode/src"),
                "tc-events": path.resolve(__dirname, "vendor/TeaEventBus/src/index.ts"),
                "tc-services": path.resolve(__dirname, "vendor/TeaClientServices/src/index.ts"),
            },
            fallback: {
                stream: "stream-browserify",
                crypto: "crypto-browserify",
                buffer: "buffer"
            }
        },
        externals: [
            {"tc-loader": "window loader"}
        ],
        output: {
            filename: developmentMode ? "js/[name].[contenthash].js" : "js/[contenthash].js",
            chunkFilename: developmentMode ? "js/[name].[contenthash].js" : "js/[contenthash].js",
            path: path.resolve(__dirname, "dist"),
            publicPath: "/"
        },
        performance: {
            hints: false
        },
        optimization: {
            splitChunks: {
                chunks: "all",
                maxSize: 512 * 1024
            },
            minimize: !developmentMode,
            minimizer: [
                new TerserPlugin(),
                new CssMinimizerPlugin()
            ]
        },
        devServer: {
            static: {
                directory: path.join(__dirname, "dist"),
                publicPath: "/",
                watch: false,
            },
            devMiddleware: {
                publicPath: "/",
                writeToDisk: true,
            },
            compress: true,

            /* hot dosn't work because of our loader */
            hot: false,

            liveReload: false,
            client: false,

            host: "0.0.0.0",
            server: process.env["serve_https"] === "1" ? "https" : "http"
        },
    };
};