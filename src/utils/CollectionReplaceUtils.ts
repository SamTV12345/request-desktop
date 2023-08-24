import {CollectionDefinition, ItemDefinition, ItemGroupDefinition} from "postman-collection";
import {CollectionDefinitionExtended} from "../store/store";
import {isItemGroupDefinition} from "../components/bareComponents/SidebarAccordeon";

export const replaceItem = (collections: ItemGroupDefinition|CollectionDefinition, collectionToReplace: ItemDefinition | ItemGroupDefinition): CollectionDefinitionExtended|ItemDefinition => {
   // Update the nested item recursively
    const updatedItems = collections.item?.map((item) => {
        if (item.id === collectionToReplace.id) {
            return collectionToReplace
        }
        if (isItemsGroupDefinition(item)) {
            return replaceItem(item, collectionToReplace)
        }
        return item
    })
    return updatedItems ? Object.assign({...collections}, {item: updatedItems}) : collections
}


export const addNewItem = (collections: ItemGroupDefinition|CollectionDefinition, idOfParent:string, itemToAdd: ItemDefinition | ItemGroupDefinition): CollectionDefinitionExtended|ItemDefinition => {

    if(isCollectionDefinition(collections)){
        return {
            ...collections,
            item: [...collections.item?collections.item:[], itemToAdd]
        }
    }

    const updatedItems = collections.item?.map((item) => {
        if (isItemsGroupDefinition(item) && item.id === idOfParent) {
            return {
                ...item,
                item: [...item.item?item.item:[], itemToAdd]
            }
        }
        return item
    })
    return updatedItems ? Object.assign({...collections}, {item: updatedItems}) : collections
}

export const deleteItem = (collections: ItemGroupDefinition|CollectionDefinition, idOfItemToDelete:string): CollectionDefinitionExtended|ItemDefinition => {
    const items = collections.item?.filter((child)=>child.id !== idOfItemToDelete).map((item) => {

       if (isItemGroupDefinition(item)){
           const children = item.item?.filter((child)=>child.id !== idOfItemToDelete)
              return Object.assign({...item}, {item: children})
       }
    }).filter((item)=>item !== undefined) as CollectionDefinitionExtended|ItemDefinition

    return Object.assign({...collections}, {item: items})
}

const isItemsGroupDefinition = (item: ItemDefinition|ItemGroupDefinition): item is ItemGroupDefinition => {
    return (item as ItemGroupDefinition).item !== undefined
}

const isCollectionDefinition = (item: ItemDefinition|CollectionDefinition): item is CollectionDefinition => {
    return "info" in item && item.info !== undefined
}
