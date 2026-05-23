import {MenuBarEntry, MenuBarEntryNormal} from "tc-shared/ui/frames/menu-bar";
import * as React from "react";
import {useEffect, useRef, useState} from "react";
import {ClientIconRenderer} from "tc-shared/ui/react-elements/Icons";
import {RemoteIconRenderer} from "tc-shared/ui/react-elements/Icon";
import {getIconManager} from "tc-shared/file/Icons";

const cssStyle = require("./Renderer.scss");

const SubMenuRenderer = (props: { entry: MenuBarEntryNormal }) => {
    if(!props.entry.children) { return null; }

    return (
        <div className={cssStyle.subMenu}>
            {props.entry.children.map(child => {
                if(child.type === "separator") {
                    return <hr key={child.uniqueId} />
                } else {
                    return <EntryRenderer entry={child} key={child.uniqueId} />;
                }
            })}
        </div>
    );
};

const MenuItemRenderer = (props: { entry: MenuBarEntryNormal }) => {
    let icon;
    if(typeof props.entry.icon === "string") {
        icon = <ClientIconRenderer icon={props.entry.icon} key={"client-icon"} />;
    } else if(typeof props.entry.icon === "object") {
        let remoteIcon = getIconManager().resolveIcon(props.entry.icon.iconId, props.entry.icon.serverUniqueId, props.entry.icon.handlerId);
        icon = <RemoteIconRenderer icon={remoteIcon} key={"remote-icon"} />;
    }

    return (
        <div className={cssStyle.menuItem} onClick={props.entry.click}>
            <div className={cssStyle.containerIcon}>{icon}</div>
            <div className={cssStyle.containerLabel}>{props.entry.label}</div>
        </div>
    );
}

const EntryRenderer = React.memo((props: { entry: MenuBarEntryNormal }) => {
    let classList = [cssStyle.containerMenuItem, cssStyle.typeSide];
    if(props.entry.children?.length) {
        classList.push(cssStyle.subEntries);
    }
    if(props.entry.disabled) {
        classList.push(cssStyle.disabled);
    }

    if(typeof props.entry.visible === "boolean" && !props.entry.visible) {
        return null;
    }

    return (
        <div className={classList.join(" ")}>
            <MenuItemRenderer entry={props.entry} />
            <SubMenuRenderer entry={props.entry} />
        </div>
    );
});

const MainEntryRenderer = React.memo((props: { entry: MenuBarEntry }) => {
    const [ active, setActive ] = useState(false);
    const rootRef = useRef<HTMLDivElement>(null);

    const entry = props.entry.type === "normal" ? props.entry : undefined;
    const visible = !!entry && !(typeof entry.visible === "boolean" && !entry.visible);

    let classList = [cssStyle.containerMenuItem];
    if(entry?.children?.length) {
        classList.push(cssStyle.subEntries);
    }
    if(entry?.disabled) {
        classList.push(cssStyle.disabled);
    }
    if(active) {
        classList.push(cssStyle.active);
    }

    useEffect(() => {
        if(!active) { return; }

        const pointerListener = (event: PointerEvent) => {
            if(rootRef.current?.contains(event.target as Node)) {
                return;
            }

            setActive(false);
        };

        const keyListener = (event: KeyboardEvent) => {
            if(event.key !== "Escape") {
                return;
            }

            setActive(false);
        };

        document.addEventListener("pointerdown", pointerListener);
        document.addEventListener("keydown", keyListener);
        return () => {
            document.removeEventListener("pointerdown", pointerListener);
            document.removeEventListener("keydown", keyListener);
        };
    }, [ active ])

    if(!visible || !entry) {
        return null;
    }

    return (
        <div className={classList.join(" ")}
             ref={rootRef}
             onClick={() => setActive(true)}
        >
            <MenuItemRenderer entry={entry} />
            <SubMenuRenderer entry={entry} />
        </div>
    );
});

export const MenuBarRenderer = (props: { items: MenuBarEntry[] }) => {
    return (
        <React.Fragment>
            {props.items.map(item => <MainEntryRenderer entry={item} key={item.uniqueId} />)}
        </React.Fragment>
    );
}