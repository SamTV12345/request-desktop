import {create} from 'zustand'
import {CollectionDefinition, ItemDefinition} from 'postman-collection'
interface APIState {
    collections:CollectionDefinition[]
    addCollection: (by: CollectionDefinition) => void,
    addCollections: (by: CollectionDefinition[]) => void,
    setCollections: (by: CollectionDefinition[]) => void,
    currentItem: ItemDefinition| undefined,
    setCurrentItem: (by: ItemDefinition) => void,
}

export const useAPIStore = create<APIState>()((set) => ({
    collections: [],
    currentItem: undefined,
    setCurrentItem: (currentItem: ItemDefinition) => set({currentItem}),
    setCollections: (collections: CollectionDefinition[]) => set({collections}),
    addCollection: (collection: CollectionDefinition) => set((state) => ({collections: [...state.collections, collection]})),
    addCollections: (collection: CollectionDefinition[]) => set((state) => ({collections: [...state.collections, ...collection]})),
    changeItem: (item: ItemDefinition) => set((state) => ({currentItem: item})),
}))
