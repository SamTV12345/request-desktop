import {useAPIStore} from "../store/store";
import {RequestMethod} from "./RequestMethod";
import {QueryParam} from "./QueryParam";
import {QueryParamDefinition, UrlDefinition} from "postman-collection";
import {useMemo} from "react";
import { invoke } from "@tauri-apps/api/tauri";

type ContentModelProps = {

}

export const ContentModel = ()=>{
    const item = useAPIStore(state=>state.currentItem)
    const url = useMemo(()=>{
        if (!item?.request){
            return ""
        }
        let urls = (item?.request.url as UrlDefinition).query as QueryParamDefinition[]
        const host = (item?.request?.url as UrlDefinition).host[0]
        let searchParams = ""
        if (urls){
            urls = urls.filter(v=>!v.disabled)
            urls.forEach(url=>{
                if (searchParams.length===0){
                    searchParams+="?"+url.key+"="+url.value
                }
                else{
                    searchParams+="&"+url.key+"="+url.value
                }
            })
        }


        return host+searchParams
    },[item?.request])


    return <div className="">
        {item?.name}
        {item && item.request &&<div><div className="link-input">
            <RequestMethod value={item}/>
            <input value={url} className="" onChange={(v)=>{}}/>
            <button onClick={()=>{
                invoke("do_request", {item: item})
                    .then(c=>console.log(c))
                    .catch(e=>console.log(e))
            }}>Senden</button>
        </div>
            {item&&(item.request.url as UrlDefinition).query&&<QueryParam/>}
        </div>}
    </div>
}
