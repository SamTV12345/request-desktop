import {CSSProperties, FC, PropsWithChildren} from "react";
import {ResizeDirection} from "./ResizableEnvironment";
import Resizer from "./Resizer";
import ResizableContext from "./ResizableContext";

type ResizableBoxProps = {
    direction: ResizeDirection,
    initialSize: number,
    className?: string,
    style?: CSSProperties,
    intialSize?: number
}

const ResizableBox: FC<PropsWithChildren<ResizableBoxProps>> = ({children, style,initialSize, className, direction}) => {
    const appliedStyle: CSSProperties = {
        position: "relative",
        ...style
    }

    return (
        <ResizableContext<HTMLDivElement> direction={direction} initialSize={initialSize} render={(size, targetRef) => (
            <div style={{
                ...appliedStyle,
                ...((direction === "top" || direction === "bottom") ? {height: size} : {width: size})
            }} ref={targetRef} className={className}>
                {children}
                <Resizer />
            </div>
        )} />
    )
}

export default ResizableBox
