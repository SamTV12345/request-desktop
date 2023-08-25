import {useForm} from "react-hook-form";
import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "../../store/store";
import {isItemGroupDefinition} from "../bareComponents/SidebarAccordeon";
import {replaceItem} from "../../utils/CollectionReplaceUtils";
import {ItemDefinition, ItemGroupDefinition} from "postman-collection";


type BasicAuthenticationData = {
    username: string,
    password: string
}

export const BasicAuthentication = ()=>{
    const currentCollection = useAPIStore(state=>state.currentCollection) as CollectionDefinitionExtended
    const updateCollection = useAPIStore(state=>state.setCurrentCollection)
    const saveCollection = useAPIStore(state=>state.saveCollection)
    const currentItem = useAPIStore(state=>state.currentItem)
    const updateCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const getKey:(key: string, defaultValue: string) => string = (key, defaultValue)=>{
        if (currentCollection?.type === DisplayType.COLLECTION_TYPE) {
            const filteredCollection = currentCollection?.auth?.basic?.filter((v) => v.key === key)
            if (filteredCollection === undefined || !filteredCollection[0]) {
                return defaultValue
            }
            return filteredCollection[0].value
        }
        else if (isItemGroupDefinition(currentItem)) {
            const filteredCollection =  currentItem!.auth?.basic?.filter((v)=>v.key === key)
            if (filteredCollection === undefined || !filteredCollection[0]) {
                return defaultValue
            }
            return filteredCollection[0].value
        }
        else if (currentItem!==undefined) {
            const filteredCollection =  currentItem!.request?.auth?.basic?.filter((v)=>v.key === key)
            if (filteredCollection === undefined || !filteredCollection[0]) {
                return defaultValue
            }
            return filteredCollection[0].value
        }
        return defaultValue
    }

    const { register, handleSubmit} = useForm<BasicAuthenticationData>({
        defaultValues: {
            password: getKey('password', ""),
            username: getKey('username', "")
        },
        mode: "onBlur"
    })

    const populateBasicAuth = (data: BasicAuthenticationData)=>{
        const newAuth = [{
            key: "username",
            type: "string",
            value: data.username

        },
            {
                key: "password",
                type: "string",
                value: data.password
            }
        ]

        if(currentCollection?.type === DisplayType.COLLECTION_TYPE){
            const clonedCollection:CollectionDefinitionExtended = {
                ...currentCollection!,
                auth: {
                    ...currentCollection?.auth,
                    type: "basic",
                    basic: newAuth
                }
            }
            updateCollection(clonedCollection)
            saveCollection()
            return
        }
        else if(isItemGroupDefinition(currentItem)){
            const clonedItem :ItemGroupDefinition= {
                ...currentItem!,
                auth: {
                    ...currentItem?.auth,
                    type: "basic",
                    basic: newAuth
                }
            }
            const updatedCollection = replaceItem(currentCollection,clonedItem) as CollectionDefinitionExtended
            updateCollection(updatedCollection)
            updateCurrentItem(clonedItem)
            saveCollection()
            return
        }
        else {
            const clonedItem: ItemDefinition = {
                ...currentItem!,
                request: {
                    ...currentItem?.request!,
                    auth: {
                        ...currentItem?.request?.auth,
                        type: "basic",
                        basic: newAuth
                    }
                }
            }
            const updatedCollection = replaceItem(currentCollection, clonedItem) as CollectionDefinitionExtended
            updateCollection(updatedCollection)
            updateCurrentItem(clonedItem)
            saveCollection()
            return
        }

    }

    return <form className="grid grid-cols-2 gap-5 text-white" onBlur={handleSubmit(populateBasicAuth)}>
        <label>Username:</label>
        <input {...register('username')} className="bg-basecol p-1"/>
        <label>Password:</label>
        <input {...register('password')} className="bg-basecol p-1"/>
    </form>
}
