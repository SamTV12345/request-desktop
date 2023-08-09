import {MouseEvent} from "react";

export type ResizeDirection = "top" | "bottom" | "left" | "right"

/**
 * State of the resizing mechanism.
 *
 * This state includes whether an element is being resized and what the original size was.
 */
type ResizableContextState = {
    /**
     * Original size of the container when it is started to be dragged.
     */
    originalSize: number,

    /**
     * Original position of the cursor where the dragging was started.
     */
    originalCursorPos: number,

    /**
     * Direction in which the resize is performed.
     */
    resizeDirection: ResizeDirection,

    /**
     * Whether a container is currently being dragged or not.
     */
    dragging: boolean,

    /**
     * Callback for setting the new size of the element.
     * @param newSize
     */
    updateSizeCallback: (newSize: number) => void
}

/**
 * Current state of the resizing.
 *
 * Initially, no element is being resized and there is a noop size update callback.
 */
let currentResizeState: ResizableContextState = {
    dragging: false,
    updateSizeCallback: () => {},

    resizeDirection: "top",
    originalCursorPos: 0,
    originalSize: 0
}

/**
 * Called when starting to drag an element.
 *
 * @param updateSizeCallback callback to update the element's size
 * @param resizeTarget target element that will be resized
 * @param mouseEvent the event that occurred when starting to drag
 * @param direction the direction in which the element is being resized
 */
export const startDrag = (updateSizeCallback: (newSize: number) => void, resizeTarget: HTMLElement,
                          mouseEvent: MouseEvent, direction: ResizeDirection) => {
    currentResizeState.dragging = true
    currentResizeState.updateSizeCallback = updateSizeCallback
    currentResizeState.resizeDirection = direction

    mouseEvent.preventDefault()

    if (direction === "left" || direction === "right") {
        currentResizeState.originalSize = resizeTarget.clientWidth
        currentResizeState.originalCursorPos = mouseEvent.clientX
    } else {
        currentResizeState.originalSize = resizeTarget.clientHeight
        currentResizeState.originalCursorPos = mouseEvent.clientY
    }
}

window.addEventListener("mousemove", (mouseEvent) => {
    if (!currentResizeState.dragging) {
        return
    }

    mouseEvent.preventDefault()

    if (currentResizeState.resizeDirection === "top") {
        currentResizeState.updateSizeCallback(currentResizeState.originalCursorPos - mouseEvent.clientY + currentResizeState.originalSize)
    } else if (currentResizeState.resizeDirection === "bottom") {
        currentResizeState.updateSizeCallback(mouseEvent.clientY - currentResizeState.originalCursorPos + currentResizeState.originalSize)
    } else if (currentResizeState.resizeDirection === "left") {
        currentResizeState.updateSizeCallback(currentResizeState.originalCursorPos - mouseEvent.clientX + currentResizeState.originalSize)
    } else if (currentResizeState.resizeDirection === "right") {
        currentResizeState.updateSizeCallback(mouseEvent.clientX - currentResizeState.originalCursorPos + currentResizeState.originalSize)
    }
})

window.addEventListener("mouseup", () => {
    currentResizeState.dragging = false
})
