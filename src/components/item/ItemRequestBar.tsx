import {RequestMethod} from "./RequestMethod";
import {invoke} from "@tauri-apps/api/tauri";
import {ResponseFromCall} from "../../models/ResponseFromCall";
import {useMemo} from "react";
import {CollectionDefinition, RequestAuth, Url as URLParser} from "postman-collection";
import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {replaceItem} from "../../utils/CollectionReplaceUtils";
import {useDebounce} from "../../hooks/useDebounce";
import {ExtraField} from "../../models/ExtraField";
import {getToken, getTokenByCollectionId} from "../collection_authorization/TokenManagerService";
import {isTokenExpired} from "../../utils/utils";
import {OAuth2SucessOutcome} from "../../models/OAuth2Outcome";

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

   const performInternalRequest = (extra_fields: any)=>{
       invoke("do_request", {item: currentItem, collection: currentCollection, extraFields: extra_fields})
           .then((c) => {
               setCurrentRequest(c as ResponseFromCall)
           })
           .catch(e => setCurrentRequest({
               body: e.toString(), cookies: {}, duration: {
                   duration: "0",
                   response_duration: "0",
               }, headers: {}, status: ""

           }))
   }


   const handleRequest = async ()=>{
       const extra_fields: ExtraField[] = []
       if (currentItem.request?.auth?.type === "oauth2") {
           await getTokenByCollectionId(currentItem.id!)
               .then((token) => {
                   if(isTokenExpired(token) && token.refresh_token){
                       console.log("token expired")
                       invoke<OAuth2SucessOutcome>("get_oauth2_token", {refreshToken: token.refresh_token, ...token.config})
                           .then(c=>{
                               extra_fields.push({key: "token", value: c.access_token})
                               performInternalRequest(extra_fields)
                           })
                           .catch(c=>{
                               console.log(c)
                           })
                   }
                   else{
                       extra_fields.push({key: "token", value: token.access_token})
                       performInternalRequest(extra_fields)
                   }

               })
       }
       else {
           performInternalRequest(extra_fields)
       }
   }

    return  <div className="request-url-section">
        <div className="border-2 border-mustard-600 p-3 rounded">
            <div className="outline-2 outline-gray-600 bg-transparent">
                <RequestMethod value={currentItem!}/>
            </div>
            <input value={url} className="bg-transparent text-white" onChange={(v) => {
                changeUrl(v.target.value)
            }} onBlur={saveCollection}/>
        </div>
        <button onClick={async () => {

            await handleRequest()
        }} className="bg-mustard-600 p-2 w-28 text-white hover:bg-mustard-500 leading-none px-4 py-3 rounded-lg shadow-[0_4px_16px_rgba(0,0,0,0.2)] hover:shadow-[0_4px_16px_theme(colors.mustard.500)] text-sm transition disabled:opacity-50 disabled:shadow-none disabled:hover:bg-mustard-600">
            Send
        </button>
    </div>
}
