import {create} from 'zustand'
import {CollectionDefinition, ItemDefinition} from 'postman-collection'
import {ResponseFromCall} from "../models/ResponseFromCall";
import {FileUpload} from "../models/FileUpload";

export enum DisplayType {
    SINGLE_TYPE,
    COLLECTION_TYPE
}

interface ItemDefinitionExtended extends ItemDefinition {
    type: DisplayType
}

interface CollectionDefinitionExtended extends CollectionDefinition {
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
}

export const useAPIStore = create<APIState>()((set) => ({
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
}))
