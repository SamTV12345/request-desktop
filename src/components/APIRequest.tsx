import {FC} from "react";

type SidebarIconProps = {
    type: string
}

export const APIRequestSidebarIcon:FC<SidebarIconProps> = ({type})=>{
    switch (type){
        case "POST":
            return <span className="request-method-post">POST</span>
        case "GET":
            return <span className="request-method-get">GET</span>
        case "PUT":
            return <span className="request-method-put">PUT</span>
        case "DELETE":
            return <span className="request-method-delete">DEL</span>
        case "PATCH":
            return <span className="request-method-patch">PATCH</span>
        case "HEAD":
            return <span className="request-method-head">HEAD</span>
        case "OPTIONS":
            return <span className="request-method-option">OPT</span>
        default:
            return <span className="request-method-get">GET</span>
    }
}
