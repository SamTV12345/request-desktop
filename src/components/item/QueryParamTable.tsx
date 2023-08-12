import * as React from 'react';
import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "../../store/store";
import {EditableTable} from "../bareComponents/EditableTable";
import {
    Collection,
    CollectionDefinition,
    ItemDefinition,
    QueryParamDefinition,
    UrlDefinition,
    VariableDefinition
} from "postman-collection";
import {useMemo} from "react";
import {replaceItem} from "../../utils/CollectionReplaceUtils";


export const ParamTable = ()=> {
    const currentItem = useAPIStore(state => state.currentItem)
    const currentCollection = useAPIStore(state => state.currentCollection)
    const saveCollection = useAPIStore(state => state.saveCollection)
    const updateCurrentCollection = useAPIStore(state => state.setCurrentCollection)

    const value = useMemo(()=>{
        if(!currentItem?.request){
            return []
        }

        if(typeof !currentItem?.request?.url === 'string'){
            return []
        }else {
            return (currentItem.request.url as UrlDefinition).query as QueryParamDefinition[]
        }
    },[currentItem?.request])
    console.log("Current colleciton123,", currentCollection)

    const disableQueryParam = (collectionId: string, disabled: boolean, index: number)=>{

        value[index].disabled = disabled
        console.log(value[index].disabled)
        const item  = {
            ...currentItem,
            request: {
                ...currentItem?.request,
                url: {
                    ...currentItem?.request?.url as UrlDefinition,
                    query: value
                }
            }
        }
        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} satisfies CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
    }

    /*
    const onKeyChange = (collectionId: string, newKey: string, index: number)=>{
        const newVariable:VariableDefinition = {
            ...collection?.variable![index],
            key: newKey
        }
        setVariable(collection!.id!,newVariable, index)
    }

    const onValueChange = (collectionId: string, newVal: string, index: number)=>{
        const newVariable:VariableDefinition = {
            ...collection?.variable![index],
            value: newVal
        }
        setVariable(collectionId,newVariable, index)
    }

    const onAdd = (collectionId: string)=>{
        addVariable(collectionId)
    }

    const onSaveOfVariable = ()=>{
        saveCollection()
    }

    const onDelete = (collectionId: string, index:number)=>{
        setVariable(collectionId,undefined, index)
    }

    const onDescriptionChange = (collectionId: string, newVal: string, index: number)=>{
        const newVariable:VariableDefinition = {
            ...collection?.variable![index],
            description: newVal
        }
        setVariable(collectionId,newVariable, index)
    }
*/

    return <EditableTable value={value} onDisabled={disableQueryParam} onKeyChange={()=>{}}
                          onValueChange={()=>{}} onDescriptionChange={()=>{}} onAdd={()=>{}} onSave={()=>{}} onDelete={()=>{}}/>
}
