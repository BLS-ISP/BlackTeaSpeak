import "react";

declare module "react" {
    function useRef<T>(): RefObject<T | undefined>;
}