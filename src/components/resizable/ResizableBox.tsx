import {CSSProperties, FC} from "react";
import {ResizeDirection} from "./ResizableEnvironment";
import Resizer from "./Resizer";
import ResizableContext from "./ResizableContext";

type ResizableBoxProps = {
    direction: ResizeDirection,
    initialSize: number,
    className?: string,
    style?: CSSProperties
}

const ResizableBox: FC<ResizableBoxProps> = ({style, className, direction}) => {
    const appliedStyle: CSSProperties = {
        position: "relative",
        ...style
    }

    return (
        <ResizableContext<HTMLDivElement> direction={direction} initialSize={200} render={(size, targetRef) => (
            <div style={{
                ...appliedStyle,
                ...((direction === "top" || direction === "bottom") ? {height: size} : {width: size})
            }} ref={targetRef} className={className}>
                <Resizer />
            </div>
        )} />
    )
}

export default ResizableBox