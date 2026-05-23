import {MenuBarDriver, MenuBarEntry, MenuBarEntryNormal} from "tc-shared/ui/frames/menu-bar";
import * as React from "react";
import * as ReactDOM from "../../../../shared/js/ui/ReactDOMCompat";
import {MenuBarRenderer} from "./Renderer";

const cssStyle = require("./Renderer.scss");

let uniqueMenuEntryId = 0;
export class WebMenuBarDriver implements MenuBarDriver {
    private readonly htmlContainer: HTMLDivElement;
    private currentEntries: MenuBarEntry[] = [];
    private currentStructureSignature = "[]";

    constructor() {
        this.htmlContainer = document.createElement("div");
        this.htmlContainer.classList.add(cssStyle.container);
        document.body.append(this.htmlContainer);
    }

    clearEntries() {
        if(this.currentEntries.length === 0) {
            return;
        }

        this.currentEntries = [];
        this.currentStructureSignature = "[]";
        this.renderMenu();
    }

    setEntries(entries: MenuBarEntry[]) {
        const nextEntries = entries.slice(0);
        nextEntries.forEach(WebMenuBarDriver.fixupUniqueIds);

        const nextSignature = WebMenuBarDriver.buildStructureSignature(nextEntries);
        if(this.currentStructureSignature === nextSignature) {
            return;
        }

        this.currentEntries = nextEntries;
        this.currentStructureSignature = nextSignature;
        this.renderMenu();
    }

    private static fixupUniqueIds(entry: MenuBarEntry) {
        if(!entry.uniqueId) {
            entry.uniqueId = "item-" + (++uniqueMenuEntryId);
        }
        if(entry.type === "normal") {
            entry.children?.forEach(WebMenuBarDriver.fixupUniqueIds);
        }
    }

    private static buildStructureSignature(entries: MenuBarEntry[]) {
        return JSON.stringify(entries.map(entry => WebMenuBarDriver.normalizeEntry(entry)));
    }

    private static normalizeEntry(entry: MenuBarEntry) {
        if(entry.type === "separator") {
            return {
                type: entry.type,
            };
        }

        return {
            type: entry.type,
            label: entry.label,
            disabled: !!entry.disabled,
            visible: typeof entry.visible === "boolean" ? entry.visible : true,
            icon: WebMenuBarDriver.normalizeIcon(entry.icon),
            children: entry.children?.map(child => WebMenuBarDriver.normalizeEntry(child)) || []
        };
    }

    private static normalizeIcon(icon: MenuBarEntryNormal["icon"]) {
        if(typeof icon === "string") {
            return { type: "client", value: icon };
        }

        if(icon && typeof icon === "object" && "iconId" in icon) {
            return {
                type: "remote",
                iconId: icon.iconId,
                serverUniqueId: icon.serverUniqueId,
                handlerId: icon.handlerId,
            };
        }

        return null;
    }

    private renderMenu() {
        ReactDOM.render(React.createElement(MenuBarRenderer, { items: this.currentEntries }), this.htmlContainer);
    }
}