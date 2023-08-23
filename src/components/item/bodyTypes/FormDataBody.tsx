import {EditableTable} from "../../bareComponents/EditableTable";
import * as React from "react";
import {CollectionDefinitionExtended, DisplayType, ItemDefinitionExtended, useAPIStore} from "../../../store/store";
import {replaceItem} from "../../../utils/CollectionReplaceUtils";
import {Collection, FormParam, FormParamDefinition, PropertyList} from "postman-collection";

export const FormDataBody = () => {
    const currentItem = useAPIStore(state => state.currentItem) as ItemDefinitionExtended
    const currentCollection = useAPIStore(state => state.currentCollection)
    const updateCurrentItem = useAPIStore(state => state.setCurrentItem)
    const updateCurrentCollection = useAPIStore(state => state.setCurrentCollection)
    const saveCollection = useAPIStore(state => state.saveCollection)
    const disableQueryParam =  (collectionId: string, disabled: boolean, index: number, itemId?: (string | undefined))=>{
        const currentItemCloned = {
            ...currentItem,
            request: {
                ...currentItem.request,
                body: {
                    ...currentItem.request?.body,
                    formdata: (currentItem.request?.body?.formdata as FormParamDefinition[])?.map((item, i) => {
                        if (i === index) {
                            item.disabled = disabled
                        }
                        return item
                    })
                }
            }}
        updateCurrentItem(currentItemCloned)
    }

    const onKeyChange = (collectionId: string, newKey: string, index: number)=>{
        const currentItemCloned = {
            ...currentItem,
            request: {
                ...currentItem.request,
                body: {
                    ...currentItem.request?.body,
                    formdata: (currentItem.request?.body?.formdata as FormParamDefinition[])?.map((item, i)=>{
                        if(i===index){
                            item.key = newKey
                        }
                        return item
                    })
                }
            }
        }
        updateCurrentItem(currentItemCloned)
    }

    const onValueChange = (collectionId: string, newValue: string, index: number)=>{
        const currentItemCloned = {
            ...currentItem,
            request: {
                ...currentItem.request,
                body: {
                    ...currentItem.request?.body,
                    formdata: currentItem.request?.header?.map((item, i)=>{
                        if(i===index){
                            item.value = newValue
                        }
                        return item
                    })
                }

            }
        }
        updateCurrentItem(currentItemCloned)
    }

    const onDescriptionChange = (collectionId: string, newDescription: string, index: number)=>{
        const currentItemCloned = {
            ...currentItem,
            request: {
                ...currentItem.request,
                body:{
                ...currentItem.request?.body,
                    formdata: (currentItem.request?.body?.formdata as FormParamDefinition[])?.map((item, i)=>{
                        if(i===index){
                            item.description = newDescription
                        }
                        return item
                    })
                }
            }
        }
        updateCurrentItem(currentItemCloned)
    }

    const onAdd = (collectionId: string)=>{
        let formdata:FormParamDefinition[] | undefined = currentItem.request?.body?.formdata as FormParamDefinition[]
        if(!formdata){
            formdata = []
        }

        formdata!.push({
            value: 'Neuer-Wert',
            key: 'Neuer-Key',
            disabled: false
        })
        const currentItemCloned:ItemDefinitionExtended = {
            ...currentItem,
            request: {
                ...currentItem.request!,
                body: {
                    ...currentItem.request?.body,
                    formdata: formdata,
                    mode: "formdata"
                }
                }
            }
        updateCurrentItem(currentItemCloned)
    }

    const saveCollectionToDB = ()=>{
        const newCollection = replaceItem(currentCollection as CollectionDefinitionExtended, currentItem) as CollectionDefinitionExtended
        updateCurrentCollection(newCollection)
        saveCollection()
    }

    const onDelete = (collectionId: string, index: number)=>{
        const currentItemCloned = {
            ...currentItem,
            request: {
                ...currentItem.request,
                header: currentItem.request?.header?.filter(( _,i)=>{
                    return i!==index
                })
            }
        }
        updateCurrentItem(currentItemCloned)
    }

    return  <EditableTable value={currentItem.request?.body?.formdata == undefined?[]: currentItem.request.body.formdata as FormParamDefinition[]}
                           onDisabled={disableQueryParam} onKeyChange={onKeyChange} disabled={true}
                           onValueChange={onValueChange} onDescriptionChange={onDescriptionChange}
                           onAdd={onAdd} onSave={saveCollectionToDB} onDelete={onDelete}/>
}
