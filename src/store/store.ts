import {create} from 'zustand'
import {CollectionDefinition, ItemDefinition, VariableDefinition} from 'postman-collection'
import {ResponseFromCall} from "../models/ResponseFromCall";
import {FileUpload} from "../models/FileUpload";
import {cloneElement} from "react";
import {invoke} from '@tauri-apps/api/tauri'
export enum DisplayType {
    SINGLE_TYPE,
    COLLECTION_TYPE
}

export interface ItemDefinitionExtended extends ItemDefinition {
    type: DisplayType
}

export interface CollectionDefinitionExtended extends CollectionDefinition {
    type: DisplayType
}

interface APIState {
    collections:CollectionDefinition[]
    addCollection: (by: CollectionDefinition) => void,
    addCollections: (by: CollectionDefinition[]) => void,
    setCollections: (by: CollectionDefinition[]) => void,
    currentItem: ItemDefinitionExtended| undefined,
    setCurrentItem: (by: ItemDefinitionExtended|undefined) => void,
    currentRequest: ResponseFromCall | undefined,
    setCurrentRequest: (by: ResponseFromCall) => void,
    fileToUpload: FileUpload|undefined,
    setFileToUpload: (by: FileUpload|undefined) => void
    setCurrentCollection: (by: CollectionDefinitionExtended) => void,
    currentCollection: CollectionDefinitionExtended|undefined
    /* Change variable in Top collection*/
    changeVarInCollection: (collectionId: string, newVar: VariableDefinition|undefined, indexOfValue: number)=>void,
    addVarInCollection: (collectionId: string)=>void,
    saveCollection: ()=>void
}

export const useAPIStore = create<APIState>()((set,getState) => ({
    collections: [],
    currentItem: undefined,
    currentRequest: undefined,
    setCurrentRequest: (currentRequest: ResponseFromCall) => set({currentRequest}),
    setCurrentItem: (currentItem: ItemDefinitionExtended|undefined) => set({currentItem}),
    setCollections: (collections: CollectionDefinition[]) => set({collections}),
    addCollection: (collection: CollectionDefinition) => set((state) => ({collections: [...state.collections, collection]})),
    addCollections: (collection: CollectionDefinition[]) => set((state) => ({collections: [...state.collections, ...collection]})),
    fileToUpload: undefined,
    setFileToUpload: (fileToUpload: FileUpload|undefined) => set({fileToUpload}),
    currentCollection: undefined,
    setCurrentCollection: (currenCollection: CollectionDefinitionExtended) => set(() => ({currentCollection: currenCollection})),
    changeVarInCollection: (collectionId: string, newVar:VariableDefinition|undefined, indexOfValue: number)=>{
            if(!getState().currentCollection?.variable){
                set({currentCollection: Object.assign({...getState().currentCollection!}, {variable: [newVar]})})
                return
            }
            if(!newVar){
                const newArray = getState().currentCollection?.variable!
                newArray.splice(indexOfValue, 1)
                const newState = Object.assign({...getState().currentCollection!}, {variable: newArray})
                set({currentCollection: newState})
                return
            }

            const newArray = getState().currentCollection!.variable!
            newArray[indexOfValue] = newVar

            const newState = Object.assign({...getState().currentCollection!}, {variable: newArray})
            set({currentCollection: newState})
        },
    addVarInCollection: (collectionId: string)=>{
        if(!getState().currentCollection?.variable){
            set({currentCollection: Object.assign({...getState().currentCollection!}, {variable: [{key: "Neue Variable", value: "Neuer Wert"}]})})
            return
        }
        const newArray = getState().currentCollection?.variable!
        newArray.push({key: "Neue Variable", value: "Neuer Wert"})
        const newState = Object.assign({...getState().currentCollection!}, {variable: newArray})
        set({currentCollection: newState})
    },
    saveCollection: ()=>{
        const newCollections = getState().collections
        const updatedCollections = newCollections.map((collection)=>{
            if(collection.id === getState().currentCollection?.id){
                return getState().currentCollection as CollectionDefinition
            }
            return collection
        })
        set({collections: updatedCollections})
        invoke("update_collection", {collection: getState().currentCollection!})
    }
}))
