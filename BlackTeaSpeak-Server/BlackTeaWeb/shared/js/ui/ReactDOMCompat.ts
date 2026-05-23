import * as React from "react";
import {createPortal, flushSync, unstable_batchedUpdates} from "react-dom";
import {createRoot, Root} from "react-dom/client";

type RootContainer = Element | DocumentFragment;

const mountedRoots = new WeakMap<RootContainer, Root>();

function getRoot(container: RootContainer): Root {
    let root = mountedRoots.get(container);
    if(!root) {
        root = createRoot(container);
        mountedRoots.set(container, root);
    }

    return root;
}

export function render(node: React.ReactNode, container: RootContainer, callback?: () => void) {
    const root = getRoot(container);
    flushSync(() => root.render(node));
    callback?.();
}

export function unmountComponentAtNode(container: RootContainer | null | undefined): boolean {
    if(!container) {
        return false;
    }

    const root = mountedRoots.get(container);
    if(!root) {
        return false;
    }

    flushSync(() => root.unmount());
    mountedRoots.delete(container);
    return true;
}

const ReactDOMCompat = {
    createPortal,
    flushSync,
    render,
    unmountComponentAtNode,
    unstable_batchedUpdates,
};

export {createPortal, flushSync, unstable_batchedUpdates};
export default ReactDOMCompat;