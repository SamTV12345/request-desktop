import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {useForm} from "react-hook-form";
import {ItemDefinition, ItemGroupDefinition, VariableDefinition} from "postman-collection";
import {isItemsGroupDefinition} from "../../utils/utils";
import {replaceItem} from "../../utils/CollectionReplaceUtils";
import {useMemo} from "react";

type APIKeyData = {
    key: string,
    value: string
    header: "header" | "query"
}

export const APiKeyAuthentication = ()=> {

    const getKey:(key: string, defaultValue: string, item: CollectionDefinitionExtended|ItemGroupDefinition|ItemDefinitionExtended) => string = (key, defaultValue)=>{
        if (currentCollection?.type === DisplayType.COLLECTION_TYPE || (isItemsGroupDefinition(currentItem))) {
            const filteredCollection =  currentCollection?.auth?.apikey?.filter((v)=>v.key === key)
            if(filteredCollection === undefined|| !filteredCollection[0]){
                return defaultValue
            }
            return filteredCollection[0].value
        }
        else {
            const filteredCollection =  currentItem?.request?.auth?.apikey?.filter((v)=>v.key === key)
            if(filteredCollection === undefined|| !filteredCollection[0]){
                return defaultValue
            }
            return filteredCollection[0].value
        }

    }
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const updateCollection = useAPIStore(state=>state.setCurrentCollection)
    const saveCollection = useAPIStore(state=>state.saveCollection)
    const currentItem = useAPIStore(state=>state.currentItem)
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const respectiveElement = useMemo(()=>{
        if (currentCollection?.type === DisplayType.COLLECTION_TYPE) {
            return currentCollection
        } else if (isItemsGroupDefinition(currentItem)) {
            return currentItem
        }
        else if (currentItem!==undefined) {
            return currentItem
        }
    },[currentCollection, currentItem])

    const { register, handleSubmit
    } = useForm<APIKeyData>({
        defaultValues: {
            value: getKey('value', "", respectiveElement!),
            key: getKey('key', "", respectiveElement!),
            header: getKey('in', "header", respectiveElement!) as "header" | "query"
        },
        mode: "onBlur"
    })


    const populateApiKeyAuth = (data: APIKeyData)=>{
        const newAuth = [{
            key: "key",
            type: "string",
            value: data.key

        },
            {
                key: "value",
                type: "string",
                value: data.value
            },
            {
                key: "in",
                type: "string",
                value: data.header
            }
        ] as VariableDefinition[]

        if (currentCollection?.type === DisplayType.COLLECTION_TYPE) {
            const clonedCollection: CollectionDefinitionExtended = {
                ...currentCollection!,
                auth: {
                    ...currentCollection?.auth,
                    type: "apikey",
                    apikey: newAuth
                }
            }
            updateCollection(clonedCollection!)
            saveCollection()
        } else if (isItemsGroupDefinition(currentItem)) {
            const clonedItem:ItemGroupDefinition = {
                ...currentItem,
                auth: {
                    ...currentItem.auth,
                    type: "apikey",
                    apikey: newAuth
                }
            }

            const updatedCollection = replaceItem(currentCollection!, clonedItem) as CollectionDefinitionExtended
            setCurrentItem(clonedItem)
            updateCollection(updatedCollection)
            saveCollection()
        }
        else if (currentItem!==undefined) {
            const clonedItem:ItemDefinitionExtended = {
                ...currentItem,
                request:
                    {
                        ...currentItem!.request!,
                        auth: {
                            ...currentItem!.request?.auth,
                            type: "apikey",
                            apikey: newAuth
                        }
                    }
            }
            const updatedCollection = replaceItem(currentCollection!, clonedItem) as CollectionDefinitionExtended
            updateCollection(updatedCollection!)
            setCurrentItem(clonedItem)
            saveCollection()
        }
    }

    return <form className="grid grid-cols-2 gap-5 text-white" onBlur={handleSubmit(populateApiKeyAuth)}>
        <label>Key</label>
        <div><input className="bg-basecol p-1" {...register('key')}/></div>

        <div>Value</div>
        <div><input className="bg-basecol p-1" {...register('value')}/></div>

        <div>Add to</div>
        <select className="bg-basecol" {...register('header')}>
            <option value="header">Header</option>
            <option value="query">Query Params</option>
        </select>
    </form>
}
