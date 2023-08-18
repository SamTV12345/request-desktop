import {SelectorKnob} from "../bareComponents/SelectorKnob";
import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {replaceItem} from "../../utils/CollectionReplaceUtils";
import {CollectionDefinition} from "postman-collection";


export enum BodyType {
    none="none",
    raw="raw", urlencoded="urlencoded", formdata="formdata", file="file", graphql="graphql"
}


export const BodySelector = () => {
    const selectedItem = useAPIStore(state => state.currentItem!)
    const setSelectedItem = useAPIStore(state => state.setCurrentItem)
    const currentCollection = useAPIStore(state => state.currentCollection!)
    const updateCurrentCollection = useAPIStore(state => state.setCurrentCollection)
    const saveCollection = useAPIStore(state => state.saveCollection)
    const switchBodyType = (collectionId: string, mode: string)=>{
        const item:ItemDefinitionExtended  = {
            ...selectedItem,
            request: {
                ...selectedItem?.request!,
                body: {
                    ...selectedItem?.request?.body,
                    mode: mode
                }
            },
            type: DisplayType.SINGLE_TYPE
        }
        setSelectedItem(item)
        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} as CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
        saveCollection()
    }

    return <div className="flex gap-6"><SelectorKnob value={selectedItem.request!.body === undefined||selectedItem.request?.body?.mode === BodyType.none} onChange={(v)=>{
        if(v){
            switchBodyType(currentCollection.id!,BodyType.none)
        }
    }} label={"Kein"}/>
        <SelectorKnob value={selectedItem.request!.body?.mode == BodyType.raw} onChange={(v)=>{
            if(v){
                console.log("Switching up")
                switchBodyType(currentCollection.id!,BodyType.raw)
            }
        }} label={"Raw"}/>
        <SelectorKnob value={selectedItem.request!.body?.mode === BodyType.formdata} onChange={(v)=>{
            if(v){
                switchBodyType(currentCollection.id!,BodyType.formdata)
            }
        }} label={"form-data"}/>
        <SelectorKnob value={selectedItem.request!.body?.mode === BodyType.urlencoded} onChange={(v)=>{
            if(v){
                switchBodyType(currentCollection.id!,BodyType.urlencoded)
            }
        }} label={"x-www-form-urlencoded"}/>
        <SelectorKnob value={selectedItem.request!.body?.mode === BodyType.file} onChange={(v)=>{
            if(v){
                switchBodyType(currentCollection.id!,BodyType.file)
            }
        }} label={"binär"}/>
        </div>
}
