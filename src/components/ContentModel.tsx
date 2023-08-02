import {useAPIStore} from "../store/store";
import {RequestMethod} from "./RequestMethod";
import {QueryParam} from "./QueryParam";
import {QueryParamDefinition, UrlDefinition} from "postman-collection";
import {useMemo} from "react";
import { invoke } from "@tauri-apps/api/tauri";
import {ResponseFromCall} from "../models/ResponseFromCall";
import SyntaxHighlighter from 'react-syntax-highlighter';

type ContentModelProps = {

}

export const ContentModel = ()=>{
    const item = useAPIStore(state=>state.currentItem)
    const currentRequest = useAPIStore(state=>state.currentRequest)
    const setCurrentRequest = useAPIStore(state=>state.setCurrentRequest)
    const currentCollection = useAPIStore(state=>state.currentCollection)

    const language = useMemo(()=>{
        if (currentRequest == undefined){
            return ""
        }
        // @ts-ignore
        return currentRequest.headers&&currentRequest.headers["content-type"]
    } ,[currentRequest])

    const url = useMemo(()=>{
        if (!item?.request){
            return ""
        }
        let urls = (item?.request.url as UrlDefinition).query as QueryParamDefinition[]
        // @ts-ignore
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


    return <div className="" style={{}}>
        {item?.name}
        {item && item.request &&<div>
            <div className="link-input">
            <RequestMethod value={item}/>
            <input value={url} className="" onChange={(v)=>{}}/>
            <button onClick={()=>{
                console.log(currentCollection)
                invoke("do_request", {item: item, collection:currentCollection})
                    .then((c)=>{
                        setCurrentRequest(c as ResponseFromCall)
                    })
                    .catch(e=>console.log(e))
            }}>Senden</button>
        </div>
            {item&&(item.request.url as UrlDefinition).query&&<QueryParam/>}
            {currentRequest &&<SyntaxHighlighter language={language}
                                                 customStyle={{width: '900px', height: '500px', overflow: "scroll"}}>
                {currentRequest.body}</SyntaxHighlighter>}
        </div>}
    </div>
}
