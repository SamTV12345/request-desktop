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


    const onKeyChange = (collectionId: string, newKey: string, index: number)=>{
        value[index].key = newKey
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


    const onValueChange = (collectionId: string, newVal: string, index: number)=>{
        value[index].value = newVal
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

    const onAdd = (collectionId: string)=>{
        value.push({
            value: 'Neuer Wert',
            key: 'Neuer Key',
            disabled: false
        })
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

    const onDelete = (collectionId: string, index:number)=>{
        const resultingValue = value.splice(index,1)
        const item  = {
            ...currentItem,
            request: {
                ...currentItem?.request,
                url: {
                    ...currentItem?.request?.url as UrlDefinition,
                    query: resultingValue
                }
            }
        }
        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} satisfies CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
    }

    const onDescriptionChange = (collectionId: string, newVal: string, index: number)=>{
        value[index].description = newVal
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

    return <EditableTable value={value} onDisabled={disableQueryParam} onKeyChange={onKeyChange}
                          onValueChange={onValueChange} onDescriptionChange={onDescriptionChange} onAdd={onAdd} onSave={saveCollection} onDelete={onDelete}/>
}
