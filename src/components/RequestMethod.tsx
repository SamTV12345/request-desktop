import {FC} from "react";
import {ItemDefinition} from "postman-collection";
import {useAPIStore} from "../store/store";

type RequestMethodProps = {
    value: ItemDefinition
}

export const RequestMethod:FC<RequestMethodProps> = ({value})=>{
    const setCurrentITem = useAPIStore(state=>state.setCurrentItem)
    return <select value={value.request!.method} onChange={(v)=>{
        value.request!.method=v.target.value as "GET" | "POST" | "PUT" | "DELETE"
        setCurrentITem({...value, request: value.request})
    }}>
        <option value="GET">GET</option>
        <option value="POST">POST</option>
        <option value="PUT">PUT</option>
        <option value="DELETE">DELETE</option>
    </select>
}
