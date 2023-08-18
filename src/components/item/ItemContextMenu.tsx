import * as ContextMenu from "@radix-ui/react-context-menu";

export const ItemContextMenu = ()=>{
    return <>
        <ContextMenu.Item className=" p-1 cursor-pointer">Open in Tab</ContextMenu.Item>
        <ContextMenu.Item className=" p-1 cursor-pointer">Rename</ContextMenu.Item>
        <ContextMenu.Item className=" p-1 cursor-pointer">Duplicate</ContextMenu.Item>
        <ContextMenu.Item className=" p-1 cursor-pointer">Delete</ContextMenu.Item>

    </>
}
