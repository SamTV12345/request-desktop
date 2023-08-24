import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../../store/store";
import {Editor} from "../Editor";
import {useDebounce} from "../../../hooks/useDebounce";
import {useState} from "react";
import {replaceItem} from "../../../utils/CollectionReplaceUtils";

export const RawBody = () => {
    const item = useAPIStore(state => (state.currentItem as ItemDefinitionExtended))
    const setItem = useAPIStore(state => state.setCurrentItem)
    const [rawBody, setRawBody] = useState<string>(()=>item.request!.body!.raw||"")
    const updateCurrentCollection = useAPIStore(state => state.setCurrentCollection)
    const saveCollection = useAPIStore(state => state.saveCollection)
    const currentCollection = useAPIStore(state => state.currentCollection)
    const changeBody = (v: string) => {
        const currentItemCloned: ItemDefinitionExtended = {
            ...item,
            request: {
                ...item.request!,
                body: {
                    ...item.request?.body!,
                    raw: v
                }
            }
        }
        setItem(currentItemCloned)
        const newCollection = replaceItem(currentCollection!, currentItemCloned)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} as CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
        saveCollection()
    }

    useDebounce(() => {
        changeBody(rawBody)
    }, 1000, [rawBody])

    return <Editor key="raw-editor" onChange={(v) => setRawBody(v!)} value={rawBody} readonly={false} mode="json"/>
}
