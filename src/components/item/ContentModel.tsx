import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {RequestMethod} from "./RequestMethod";
import {QueryParam} from "./QueryParam";
import {CollectionDefinition, Url as URLParser} from "postman-collection";
import {useMemo} from "react";
import {invoke} from "@tauri-apps/api/tauri";
import {ResponseFromCall} from "../../models/ResponseFromCall";
import ResizableBox from "../resizable/ResizableBox";
import {ResponseBar} from "./responseBarItems/ResponseBar";
import {replaceItem} from "../../utils/CollectionReplaceUtils";
import {useDebounce} from "../../hooks/useDebounce";


export const ContentModel = () => {
    const setCurrentItem = useAPIStore(state => state.setCurrentItem)
    const currentItem = useAPIStore(state => state.currentItem)
    const currentRequest = useAPIStore(state => state.currentRequest)
    const setCurrentRequest = useAPIStore(state => state.setCurrentRequest)
    const currentCollection = useAPIStore(state => state.currentCollection)
    const updateCurrentCollection = useAPIStore(state => state.setCurrentCollection)
    const saveCollection = useAPIStore(state => state.saveCollection)


    useDebounce(()=>{
        saveCollection()
    },5000, [currentItem?.request?.url])

    const url = useMemo(() => {
        if (!currentItem?.request) {
            return ""
        }
        return new URLParser(currentItem.request.url).toString(true)
    }, [currentItem?.request])

    const changeUrl = (url: string) => {
        const urlDef = URLParser.parse(url)
        const item:ItemDefinitionExtended  = {
            ...currentItem,
            request: {
                ...currentItem?.request,
                url: urlDef
            },
            type: DisplayType.SINGLE_TYPE
        }

        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} satisfies CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
        setCurrentItem(item)
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
