import {
    createContext,
    MouseEvent,
    ReactElement,
    RefObject,
    useCallback,
    useMemo,
    useRef,
    useState
} from "react";
import {ResizeDirection, startDrag} from "./ResizableEnvironment";

type ResizableContextProps<T> = {
    direction: ResizeDirection,
    render: (size: number, targetRef: RefObject<T>) => ReactElement,
    initialSize: number
}

type ResizableContextState = {
    direction: ResizeDirection,
    startDragCallback: (mouseEvent: MouseEvent) => void,
}

export const ResizableReactContext = createContext<ResizableContextState>({
    direction: "top",
    startDragCallback: () => {
    }
})

const ResizableContext = <T extends HTMLElement>({direction, render, initialSize}: ResizableContextProps<T>) => {
    const targetRef = useRef<T>(null)

    const [size, setSize] = useState(initialSize)

    const startDragCallback = useCallback((e: MouseEvent) => {
        if (targetRef.current) {
            startDrag(setSize, targetRef.current, e, direction)
        }
    }, [direction])

    const contextValue = useMemo(() => ({
        direction: direction,
        startDragCallback
    }), [direction])

    return (
        <ResizableReactContext.Provider value={contextValue}>
            {render(size, targetRef)}
        </ResizableReactContext.Provider>
    )
}

export default ResizableContext