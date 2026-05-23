import {Registry} from "tc-shared/events";
import {ChannelTreeUIEvents} from "tc-shared/ui/tree/Definitions";
import * as React from "react";
import {useEffect, useMemo, useRef} from "react";
import {ChannelTreeView, PopoutButton} from "tc-shared/ui/tree/RendererView";
import {RDPChannel, RDPChannelTree} from "./RendererDataProvider";

const viewStyle = require("./View.scss");

export const ChannelTreeRenderer = (props: { handlerId: string, events: Registry<ChannelTreeUIEvents> }) => {
    const dataProvider = useMemo(() => new RDPChannelTree(props.events, props.handlerId), [ props.events, props.handlerId ]);
    useEffect(() => {
        dataProvider.initialize();
        return () => dataProvider.destroy();
    }, [ dataProvider ]);

    return <ContainerView tree={dataProvider} events={props.events} />;
}

const ContainerView = (props: { tree: RDPChannelTree, events: Registry<ChannelTreeUIEvents> }) => {
    const refContainer = useRef<HTMLDivElement>();
    const focusWithin = useRef(false);

    useEffect(() => {
        const mouseDownListener = event => {
            let target = event.target as HTMLElement;
            while(target && target !== refContainer.current) { target = target.parentElement; }

            if(focusWithin.current && target === refContainer.current) {
                refContainer.current?.focus();
            }
        };
        document.addEventListener("mousedown", mouseDownListener);

        const keyListener = event => {
            if(!focusWithin.current) { return; }

            if(event.key === "ArrowUp") {
                event.preventDefault();
                props.tree.selection.selectNext(true, "up");
            } else if(event.key === "ArrowDown") {
                event.preventDefault();
                props.tree.selection.selectNext(true, "down");
            } else if(event.key === "Enter") {
                event.preventDefault();

                const selectedEntries = props.tree.selection.selectedEntries;
                if(selectedEntries.length !== 1) { return; }
                if(!(selectedEntries[0] instanceof RDPChannel)) { return; }
                props.events.fire("action_channel_join", { treeEntryId: selectedEntries[0].entryId });
            }
        };
        document.addEventListener("keydown", keyListener);

        return () => {
            document.removeEventListener("mousedown", mouseDownListener);
            document.removeEventListener("keydown", keyListener);
        }
    }, [ props.events, props.tree ]);

    return (
        <div
            className={viewStyle.treeContainer}
            ref={refContainer}
            onBlur={() => focusWithin.current = false}
            onFocus={() => focusWithin.current = true}
            tabIndex={1}
        >
            <ChannelTreeView events={props.events} dataProvider={props.tree} ref={props.tree.refTree} />
            <PopoutButton tree={props.tree} ref={props.tree.refPopoutButton} />
        </div>
    )
}