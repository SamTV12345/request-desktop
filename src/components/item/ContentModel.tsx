import {useAPIStore} from "../../store/store";
import {RequestMethod} from "./RequestMethod";
import {QueryParam} from "./QueryParam";
import {QueryParamDefinition, UrlDefinition} from "postman-collection";
import {useMemo} from "react";
import {invoke} from "@tauri-apps/api/tauri";
import {ResponseFromCall} from "../../models/ResponseFromCall";
import SyntaxHighlighter from 'react-syntax-highlighter';
import ResizableBox from "../resizable/ResizableBox";
import {Editor} from "./Editor";


export const ContentModel = () => {
    const item = useAPIStore(state => state.currentItem)
    const currentRequest = useAPIStore(state => state.currentRequest)
    const setCurrentRequest = useAPIStore(state => state.setCurrentRequest)
    const currentCollection = useAPIStore(state => state.currentCollection)

    const url = useMemo(() => {
        if (!item?.request) {
            return ""
        }
        let urls = (item?.request.url as UrlDefinition).query as QueryParamDefinition[]
        const host = (item?.request?.url as UrlDefinition).host
        let searchParams = ""
        if (urls) {
            urls = urls.filter(v => !v.disabled)
            urls.forEach(url => {
                if (searchParams.length === 0) {
                    searchParams += "?" + url.key + "=" + url.value
                } else {
                    searchParams += "&" + url.key + "=" + url.value
                }
            })
        }
        return host + searchParams
    }, [item?.request])


    return (
        <div className="request-view">
            <div className="request-name-section">
                {item?.name}
            </div>
            {item && item.request && <>
                <div className="request-url-section">
                    <div className="border-2 border-mustard-600 p-3 rounded">
                        <div className="outline-2 outline-gray-600 bg-transparent">
                            <RequestMethod value={item}/>
                        </div>
                        <input value={url} className="" onChange={(v) => {
                        }}/>
                    </div>
                    <button onClick={() => {
                        invoke("do_request", {item: item, collection: currentCollection})
                            .then((c) => {
                                setCurrentRequest(c as ResponseFromCall)
                            })
                            .catch(e => console.log(e))
                    }} className="bg-mustard-600 p-2 rounded w-28">
                        Senden
                    </button>
                </div>
                <QueryParam/>
                {currentRequest &&
                    <ResizableBox direction={"top"} initialSize={300} className="response-section">
                        <Editor/>
                    </ResizableBox>}
            </>}
        </div>
    )
}
