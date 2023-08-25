import {useForm} from "react-hook-form";
import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../store/store";
import {ItemDefinition, ItemGroupDefinition, VariableDefinition} from "postman-collection";
import {isItemGroupDefinition} from "../bareComponents/SidebarAccordeon";
import {replaceItem} from "../../utils/CollectionReplaceUtils";


type BearerData = {
    token:string
}

export const BearerAuthentication = ()=>{
    const currentCollection = useAPIStore(state=>state.currentCollection) as CollectionDefinitionExtended
    const updateCollection = useAPIStore(state=>state.setCurrentCollection)
    const saveCollection = useAPIStore(state=>state.saveCollection)
    const currentItem = useAPIStore(state=>state.currentItem)
    const updateCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const AUTH_TYPE = "bearer"
    const getKey:(key: string, defaultValue: string) => string = (key, defaultValue)=>{
        if (currentCollection?.type === DisplayType.COLLECTION_TYPE) {
            const filteredCollection = currentCollection?.auth?.bearer?.filter((v) => v.key === key)
            if (filteredCollection === undefined || !filteredCollection[0]) {
                return defaultValue
            }
            return filteredCollection[0].value
        }
        else if (isItemGroupDefinition(currentItem)) {
            const filteredCollection =  currentItem!.auth?.bearer?.filter((v)=>v.key === key)
            if (filteredCollection === undefined || !filteredCollection[0]) {
                return defaultValue
            }
            return filteredCollection[0].value
        }
        else if (currentItem!==undefined) {
            const filteredCollection =  currentItem!.request?.auth?.bearer?.filter((v)=>v.key === key)
            if (filteredCollection === undefined || !filteredCollection[0]) {
                return defaultValue
            }
            return filteredCollection[0].value
        }
        return defaultValue
    }


    const { register, handleSubmit
    } = useForm<BearerData>({
        defaultValues: {
            token: getKey('token', "")
        },
        mode: "onBlur"
    });

    const populateBearerKeyAuth = (data: BearerData)=>{
        const newAuth = [{
            key: "token",
            type: "string",
            value: data.token
        }
        ] as VariableDefinition[]

        if(currentCollection?.type === DisplayType.COLLECTION_TYPE){
            const clonedCollection:CollectionDefinitionExtended = {
                ...currentCollection!,
                auth: {
                    ...currentCollection?.auth,
                    type: AUTH_TYPE,
                    bearer: newAuth
                }
            }
            updateCollection(clonedCollection)
            saveCollection()
            return
        }
        else if(isItemGroupDefinition(currentItem)){
            const clonedItem:ItemGroupDefinition = {
                ...currentItem!,
                auth: {
                    ...currentItem?.auth!,
                    type: AUTH_TYPE,
                    bearer: newAuth
                }
            }
            const updatedCollection = replaceItem(currentCollection,clonedItem) as CollectionDefinitionExtended
            updateCurrentItem(clonedItem)
            updateCollection(updatedCollection)
            saveCollection()
            return
        }
        else if (currentItem) {
            console.log("currentItem", currentItem, newAuth)
            const clonedItem: ItemDefinition = {
                ...currentItem!,
                request: {
                    ...currentItem?.request!,
                    auth: {
                        ...currentItem?.request?.auth,
                        type: AUTH_TYPE,
                        bearer: newAuth
                    }
                }
            }
            const updatedCollection = replaceItem(currentCollection, clonedItem) as CollectionDefinitionExtended
            updateCurrentItem(clonedItem)
            updateCollection(updatedCollection)
            saveCollection()
            return
        }
    }


    return <form onBlur={handleSubmit(populateBearerKeyAuth)} className="grid grid-cols-2 gap-5 text-white">
        <label>Token:</label>
        <input {...register('token')} className="bg-basecol p-1"/>
    </form>
}
