import {CSSProperties, FC, MouseEvent, useContext} from "react";
import {ResizableReactContext} from "./ResizableContext";

const resizerStyle: CSSProperties = {
    position: "absolute",
    background: "none"
}

const resizerTopBottomStyle: CSSProperties = {
    ...resizerStyle,
    left: "0",
    width: "100%",
    height: "10px",
    cursor: "row-resize"
}

const resizerLeftRightStyle: CSSProperties = {
    ...resizerStyle,
    top: "0",
    height: "100%",
    width: "10px",
    cursor: "col-resize"
}

const resizerStyles = {
    top: {
        ...resizerTopBottomStyle,
        top: "-5px",
    },
    bottom: {
        ...resizerTopBottomStyle,
        bottom: "-5px",
    },
    left: {
        ...resizerLeftRightStyle,
        left: "-5px",
    },
    right: {
        ...resizerLeftRightStyle,
        right: "-5px",
    }
}

const Resizer: FC = () => {
    const resizableContext = useContext(ResizableReactContext)

    const handleMouseDown = (e: MouseEvent) => {
        resizableContext.startDragCallback(e)
    }

    return (
        <div style={resizerStyles[resizableContext.direction]} onMouseDown={handleMouseDown}/>
    )
}

export default Resizer