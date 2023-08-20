import {RequestMethod} from "./RequestMethod";
import {invoke} from "@tauri-apps/api/tauri";
import {ResponseFromCall} from "../../models/ResponseFromCall";
import {useMemo} from "react";
import {CollectionDefinition, Url as URLParser} from "postman-collection";
import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {replaceItem} from "../../utils/CollectionReplaceUtils";
import {useDebounce} from "../../hooks/useDebounce";

export const ItemRequestBar = ()=>{
    const setCurrentRequest = useAPIStore(state => state.setCurrentRequest)
    const currentCollection = useAPIStore(state => state.currentCollection)
    const updateCurrentCollection = useAPIStore(state => state.setCurrentCollection)
    const setCurrentItem = useAPIStore(state => state.setCurrentItem)
    const saveCollection = useAPIStore(state => state.saveCollection)
    const currentItem = useAPIStore(state => state.currentItem) as ItemDefinitionExtended

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

        const newCollection = replaceItem(currentCollection as CollectionDefinitionExtended, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} as CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
        setCurrentItem(item)
    }


    return  <div className="request-url-section">
        <div className="border-2 border-mustard-600 p-3 rounded">
            <div className="outline-2 outline-gray-600 bg-transparent">
                <RequestMethod value={currentItem!}/>
            </div>
            <input value={url} className="bg-transparent text-white" onChange={(v) => {
                changeUrl(v.target.value)
            }}/>
        </div>
        <button onClick={() => {
            invoke("do_request", {item: currentItem, collection: currentCollection})
                .then((c) => {
                    setCurrentRequest(c as ResponseFromCall)
                })
                .catch(e => console.log(e))
        }} className="bg-mustard-600 p-2 w-28 text-white hover:bg-mustard-500 leading-none px-4 py-3 rounded-lg shadow-[0_4px_16px_rgba(0,0,0,0.2)] hover:shadow-[0_4px_16px_theme(colors.mustard.500)] text-sm transition disabled:opacity-50 disabled:shadow-none disabled:hover:bg-mustard-600">
            Senden
        </button>
    </div>
}
