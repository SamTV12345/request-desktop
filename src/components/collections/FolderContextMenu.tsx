import {FC} from "react";
import {CollectionDefinitionExtended} from "../../store/store";
import * as ContextMenu from "@radix-ui/react-context-menu";
import {ItemGroupDefinition} from "postman-collection";

type CollectionContextMenuProps = {
    collection: ItemGroupDefinition
}

export const FolderContextMenu:FC<CollectionContextMenuProps> = ({collection})=>{
    return <>
        <ContextMenu.Item className="p-1 cursor-pointer">Add Request</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Add Folder</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Rename</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Duplicate</ContextMenu.Item>
        <ContextMenu.Item className="p-1 cursor-pointer">Delete</ContextMenu.Item>
    </>
}
