import {FC} from "react";

type SidebarIconProps = {
    type: string
}

export const APIRequestSidebarIcon:FC<SidebarIconProps> = ({type})=>{
    switch (type){
        case "POST":
            return <span className="request-method-post font-icons">POST</span>
        case "GET":
            return <span className="request-method-get font-icons">GET</span>
        case "PUT":
            return <span className="request-method-put font-icons">PUT</span>
        case "DELETE":
            return <span className="request-method-delete font-icons">DEL</span>
        case "PATCH":
            return <span className="request-method-patch font-icons">PATCH</span>
        case "HEAD":
            return <span className="request-method-head font-icons">HEAD</span>
        case "OPTIONS":
            return <span className="request-method-option font-icons">OPT</span>
        default:
            return <span className="request-method-get font-icons">GET</span>
    }
}
