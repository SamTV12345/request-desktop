import * as React from 'react';
import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../store/store";
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


export const HeaderTable = ()=> {
    const currentItem = useAPIStore(state => state.currentItem) as ItemDefinitionExtended
    const currentCollection = useAPIStore(state => state.currentCollection)
    const saveCollection = useAPIStore(state => state.saveCollection)
    const updateCurrentCollection = useAPIStore(state => state.setCurrentCollection)

    const headers = useMemo(()=>{
        if(!currentItem?.request||!currentItem.request.header){
            return []
        }

        return currentItem.request.header||[]
    },[currentItem?.request])

    const disableQueryParam = (collectionId: string, disabled: boolean, index: number)=>{

        headers[index].disabled = disabled
        const item  = {
            ...currentItem,
            request: {
                ...currentItem?.request,
                headers: headers
            }
        }
        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} as CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
    }


    const onKeyChange = (collectionId: string, newKey: string, index: number)=>{
        headers[index].key = newKey
        const item  = {
            ...currentItem,
            request: {
                ...currentItem?.request,
                headers: headers
            }
        }
        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE,} as CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
    }


    const onValueChange = (collectionId: string, newVal: string, index: number)=>{
        headers[index].value = newVal
        const item  = {
            ...currentItem,
            request: {
                ...currentItem?.request,
                headers: headers
            }
        }
        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} as CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
    }

    const onAdd = (collectionId: string)=>{
        headers.push({
            value: 'Neuer-Wert',
            key: 'Neuer-Key',
            disabled: false
        })
        const item  = {
            ...currentItem,
            request: {
                ...currentItem?.request,
                headers: headers
            }
        }
        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} as CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
    }

    const onDelete = (collectionId: string, index:number)=>{
        const item  = {
            ...currentItem,
            request: {
                ...currentItem?.request,
                headers: headers
            }
        }
        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} as CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
    }

    const onDescriptionChange = (collectionId: string, newVal: string, index: number)=>{
        headers[index].description = newVal
        const item  = {
            ...currentItem,
            request: {
                ...currentItem?.request,
                headers: headers
            }
        }
        const newCollection = replaceItem(currentCollection as CollectionDefinition, item)
        const newCollectionExtended = {...newCollection,type: DisplayType.SINGLE_TYPE} as CollectionDefinitionExtended
        updateCurrentCollection(newCollectionExtended)
    }

    return <EditableTable value={headers} onDisabled={disableQueryParam} onKeyChange={onKeyChange}
                                 onValueChange={onValueChange} onDescriptionChange={onDescriptionChange} onAdd={onAdd} onSave={saveCollection} onDelete={onDelete}/>
}
