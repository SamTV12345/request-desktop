import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "../../store/store";
import {RequestMethod} from "./RequestMethod";
import {QueryParam} from "./QueryParam";
import {CollectionDefinition, ItemDefinition, QueryParamDefinition, UrlDefinition} from "postman-collection";
import {useMemo} from "react";
import {invoke} from "@tauri-apps/api/tauri";
import {ResponseFromCall} from "../../models/ResponseFromCall";
import SyntaxHighlighter from 'react-syntax-highlighter';
import ResizableBox from "../resizable/ResizableBox";
import {Editor} from "./Editor";
import {ResponseBar} from "./responseBarItems/ResponseBar";
import {replaceItem} from "../../utils/CollectionReplaceUtils";


export const ContentModel = () => {
    const currentItem = useAPIStore(state => state.currentItem)
    const currentRequest = useAPIStore(state => state.currentRequest)
    const setCurrentRequest = useAPIStore(state => state.setCurrentRequest)
    const currentCollection = useAPIStore(state => state.currentCollection)
    const updateCurrentCollection = useAPIStore(state => state.setCurrentCollection)

    const url = useMemo(() => {
        if (!currentItem?.request) {
            return ""
        }
        let urls = (currentItem?.request.url as UrlDefinition).query as QueryParamDefinition[]
        const host = (currentItem?.request?.url as UrlDefinition).host
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
    }, [currentItem?.request])

    const changeUrl = (url: string) => {
        const urlParsed = new URLSearchParams(url)
        let counter = 0
        const query = ((currentItem?.request?.url!) as UrlDefinition).query as QueryParamDefinition[]
        urlParsed.forEach((value, key) => {
            query[counter] = {...query[counter],
                key: key,
                value: value
            }
        })


        const item:ItemDefinition  = {
            ...currentItem,
            request: {
                ...currentItem?.request,
                url: {
                    host: "google.com",
                    path: "/test",
                    protocol: "http",
                    ...currentItem?.request?.url as UrlDefinition,
                    query: query
                }
            }
        }
        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} satisfies CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
    }

    return (
        <div className="request-view">
            <div className="request-name-section">
                {currentItem?.name}
            </div>
            {currentItem && currentItem.request && <>
                <div className="request-url-section">
                    <div className="border-2 border-mustard-600 p-3 rounded">
                        <div className="outline-2 outline-gray-600 bg-transparent">
                            <RequestMethod value={currentItem}/>
                        </div>
                        <input value={url} className="" onChange={(v) => {
                            changeUrl(v.target.value)
                        }}/>
                    </div>
                    <button onClick={() => {
                        invoke("do_request", {item: currentItem, collection: currentCollection})
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
                        <ResponseBar/>
                    </ResizableBox>}
            </>}
        </div>
    )
}
