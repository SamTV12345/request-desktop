import {FC} from "react";
import {ItemDefinition} from "postman-collection";
import {DisplayType, useAPIStore} from "../../store/store";

type RequestMethodProps = {
    value: ItemDefinition
}

export const RequestMethod:FC<RequestMethodProps> = ({value})=>{
    const setCurrentITem = useAPIStore(state=>state.setCurrentItem)




    return <select value={value.request!.method} className={`bg-transparent request-method-${value.request?.method?.toLowerCase()}`}
                   onChange={(v)=>{
        value.request!.method=v.target.value as "GET" | "POST" | "PUT" | "DELETE"
        setCurrentITem({...value, request: value.request, type: DisplayType.SINGLE_TYPE})
    }}>
        <option value="GET" className="request-method-get font-normal bg-basecol">GET</option>
        <option value="POST" className="request-method-post font-normal bg-basecol ">POST</option>
        <option value="PUT" className="request-method-put font-normal bg-basecol">PUT</option>
        <option value="DELETE" className="request-method-delete font-normal bg-basecol">DELETE</option>
    </select>
}
