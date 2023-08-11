import {FC} from "react";
import {ItemDefinition} from "postman-collection";
import {DisplayType, useAPIStore} from "../../store/store";

type RequestMethodProps = {
    value: ItemDefinition
}

export const RequestMethod:FC<RequestMethodProps> = ({value})=>{
    const setCurrentITem = useAPIStore(state=>state.setCurrentItem)

    return <select value={value.request!.method} className="bg-transparent"
                   onChange={(v)=>{
        value.request!.method=v.target.value as "GET" | "POST" | "PUT" | "DELETE"
        setCurrentITem({...value, request: value.request, type: DisplayType.SINGLE_TYPE})
    }}>
        <option value="GET" className="request-method-get font-normal">GET</option>
        <option value="POST" className="request-method-post font-normal">POST</option>
        <option value="PUT" className="request-method-put font-normal">PUT</option>
        <option value="DELETE" className="request-method-delete font-normal">DELETE</option>
    </select>
}
