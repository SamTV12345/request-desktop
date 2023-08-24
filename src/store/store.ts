import {create} from 'zustand'
import {CollectionDefinition, ItemDefinition, ItemGroupDefinition, VariableDefinition} from 'postman-collection'
import {ResponseFromCall} from "../models/ResponseFromCall";
import {FileUpload} from "../models/FileUpload";
import {cloneElement} from "react";
import {invoke} from '@tauri-apps/api/tauri'
import {OAuth2FailureOutcome, OAuth2SucessOutcome, TokenWithKey} from "../models/OAuth2Outcome";
export enum DisplayType {
    SINGLE_TYPE,
    COLLECTION_TYPE
}

export interface ItemDefinitionExtended extends ItemDefinition {
    type: DisplayType
}

export interface CollectionDefinitionExtended extends CollectionDefinition {
    type: DisplayType,
    info: {
        id?: string | undefined
        name?: string | undefined
        version?: string | undefined;
        _postman_id: string,
        schema?: string
    }
}

interface APIState {
    collections: CollectionDefinitionExtended[]
    addCollection: (by: CollectionDefinitionExtended) => void,
    addCollections: (by: CollectionDefinitionExtended[]) => void,
    setCollections: (by: CollectionDefinitionExtended[]) => void,
    currentItem: ItemDefinitionExtended|ItemGroupDefinition| undefined,
    setCurrentItem: (by: ItemDefinitionExtended|ItemGroupDefinition|undefined) => void,
    currentRequest: ResponseFromCall | undefined,
    setCurrentRequest: (by: ResponseFromCall) => void,
    fileToUpload: FileUpload|undefined,
    setFileToUpload: (by: FileUpload|undefined) => void
    setCurrentCollection: (by: CollectionDefinitionExtended) => void,
    currentCollection: CollectionDefinitionExtended|undefined
    /* Change variable in Top collection*/
    changeVarInCollection: (collectionId: string, newVar: VariableDefinition|undefined, indexOfValue: number)=>void,
    addVarInCollection: (collectionId: string)=>void,
    saveCollection: ()=>void,
    saveGivenCollection: (collection: CollectionDefinitionExtended)=>void,
    openOAuth2Screen: boolean,
    setOAuth2Screen: (by: boolean)=>void,
    oauth2Outcome: OAuth2SucessOutcome|OAuth2FailureOutcome|undefined,
    setOAuth2Outcome: (by: OAuth2SucessOutcome|OAuth2FailureOutcome|undefined)=>void,
    openTokenManager: boolean,
    setOpenTokenManager: (by: boolean)=>void,
    tokens: TokenWithKey[],
    setTokens: (by: TokenWithKey[])=>void,
    selectedToken: TokenWithKey|undefined,
    setSelectedToken: (by: TokenWithKey|undefined)=>void,
    openNewCollectionModal: boolean,
    setOpenNewCollectionModal: (by: boolean)=>void,
    insertNewCollection: (collection: CollectionDefinitionExtended)=>void,
    newItemInserterOpen: boolean,
    setNewItemInserterOpen: (by: boolean)=>void,
    responseExtended: boolean,
    setResponseExtended: (by: boolean)=>void
}

export const useAPIStore = create<APIState>()((set,getState) => ({
    collections: [],
    currentItem: undefined,
    currentRequest: undefined,
    setCurrentRequest: (currentRequest: ResponseFromCall) => set({currentRequest}),
    setCurrentItem: (currentItem: ItemDefinitionExtended|ItemGroupDefinition|undefined) => set({currentItem}),
    setCollections: (collections: CollectionDefinitionExtended[]) => set({collections}),
    addCollection: (collection: CollectionDefinitionExtended) => set((state) => ({collections: [...state.collections, collection]})),
    addCollections: (collection: CollectionDefinitionExtended[]) => set((state) => ({collections: [...state.collections, ...collection]})),
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
            if(collection.info._postman_id === getState().currentCollection?.info._postman_id){
                return getState().currentCollection as CollectionDefinitionExtended
            }
            return collection
        })
        set({collections: updatedCollections})
        invoke("update_collection", {collection: getState().currentCollection!})
    },
    openOAuth2Screen: false,
    setOAuth2Screen: (openOAuth2Screen: boolean)=>set({openOAuth2Screen}),
    oauth2Outcome: undefined,
    setOAuth2Outcome: (oauth2Outcome: OAuth2SucessOutcome|OAuth2FailureOutcome|undefined)=>set({oauth2Outcome}),
    openTokenManager: false,
    setOpenTokenManager: (openTokenManager: boolean)=>set({openTokenManager}),
    tokens: [],
    setTokens: (tokens: TokenWithKey[])=>set({tokens}),
    selectedToken: undefined,
    setSelectedToken: (selectedToken: TokenWithKey|undefined)=>set({selectedToken}),
    openNewCollectionModal: false,
    setOpenNewCollectionModal: (openNewCollectionModal: boolean)=>set({openNewCollectionModal}),
    insertNewCollection: (collection: CollectionDefinitionExtended)=>{
        set({collections: [...getState().collections, collection]})
    },
    newItemInserterOpen: false,
    setNewItemInserterOpen: (newItemInserterOpen: boolean)=>set({newItemInserterOpen}),
    saveGivenCollection: (collection: CollectionDefinitionExtended)=>{
        const newCollections = getState().collections
        const updatedCollections = newCollections.map((collectionInState)=>{
            if(collectionInState.info._postman_id === collection.info._postman_id){
                return collection
            }
            return collectionInState
        })
        set({collections: updatedCollections})
        invoke("update_collection", {collection: collection})
    },
    responseExtended: false,
    setResponseExtended: (responseExtended: boolean)=>set({responseExtended})
}))
