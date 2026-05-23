import * as path from "node:path";
import * as config_base from "./webpack.config";

export = (env, argv?: { mode?: string }) => config_base.config(env, "web", argv).then(config => {
    Object.assign(config.entry, {
        "main-app": ["./web/app/entry-points/AppMain.ts"],
        "modal-external": ["./web/app/entry-points/ModalWindow.ts"]
    });

    Object.assign(config.resolve.alias, {
        "tc-shared": path.resolve(__dirname, "shared/js"),
    });

    return config;
});