import {CollectionDefinition, ItemDefinition, ItemGroupDefinition} from "postman-collection";
import {CollectionDefinitionExtended} from "../store/store";

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

const isItemsGroupDefinition = (item: ItemDefinition|ItemGroupDefinition): item is ItemGroupDefinition => {
    return (item as ItemGroupDefinition).item !== undefined
}
