import {create} from 'zustand'
import {CollectionDefinition, ItemDefinition} from 'postman-collection'
import {ResponseFromCall} from "../models/ResponseFromCall";
interface APIState {
    collections:CollectionDefinition[]
    addCollection: (by: CollectionDefinition) => void,
    addCollections: (by: CollectionDefinition[]) => void,
    setCollections: (by: CollectionDefinition[]) => void,
    currentItem: ItemDefinition| undefined,
    setCurrentItem: (by: ItemDefinition) => void,
    currentRequest: ResponseFromCall | undefined,
    setCurrentRequest: (by: ResponseFromCall) => void,
}

export const useAPIStore = create<APIState>()((set) => ({
    collections: [],
    currentItem: undefined,
    currentRequest: undefined,
    setCurrentRequest: (currentRequest: ResponseFromCall) => set({currentRequest}),
    setCurrentItem: (currentItem: ItemDefinition) => set({currentItem}),
    setCollections: (collections: CollectionDefinition[]) => set({collections}),
    addCollection: (collection: CollectionDefinition) => set((state) => ({collections: [...state.collections, collection]})),
    addCollections: (collection: CollectionDefinition[]) => set((state) => ({collections: [...state.collections, ...collection]})),
    changeItem: (item: ItemDefinition) => set((state) => ({currentItem: item})),
}))
