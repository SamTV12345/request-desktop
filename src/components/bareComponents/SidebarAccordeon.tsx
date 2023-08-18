import {FC} from "react";
import {CollectionDefinition, ItemDefinition, ItemGroupDefinition} from "postman-collection";
import {Accordion, AccordionContent, AccordionItem, AccordionTrigger,} from "./Accordeon"
import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "../../store/store";
import {APIRequestSidebarIcon} from "../item/APIRequest";
import {SidebarContextMenu} from "./SidebarContextMenu";
import {CollectionContextMenu} from "../collections/CollectionContextMenu";
import {FolderContextMenu} from "../collections/FolderContextMenu";
import {ItemContextMenu} from "../item/ItemContextMenu";


type SidebarAccordeonProps = {
    collection: CollectionDefinitionExtended
}

export const isItemGroupDefinition = (item: any): item is ItemGroupDefinition => {
    return 'item' in item && item.item != undefined
}

export const SidebarAccordeon:FC<SidebarAccordeonProps> = ({collection}) => {
    const initialNumber = 1

    const setCurrentCollection = useAPIStore(state=>state.setCurrentCollection)
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)

        return  <Accordion type="single" collapsible  key={collection.name+"name"} className="border-none">
            <AccordionItem value="item-1" key={collection.name+"item"} className="cursor-pointer border-none">
                    <AccordionTrigger><span onClick={()=>{
                        // @ts-ignore
                        setCurrentCollection({...collection, type: DisplayType.COLLECTION_TYPE})
                        setCurrentItem(undefined)
                    }}><SidebarContextMenu triggerLabel={collection.info?.name as string} children={<CollectionContextMenu collection={collection}/>}/></span></AccordionTrigger>
                        <AccordionContent className="">
                            <RecursiveItemGroup item={collection.item} collection={collection}/>
                        </AccordionContent>
            </AccordionItem>
    </Accordion>
}

type RecursiveItemGroupProps = {
    item: (ItemGroupDefinition | ItemDefinition)[] | undefined,
    collection: CollectionDefinition
}
export const RecursiveItemGroup:FC<RecursiveItemGroupProps> = ({item, collection})=>{
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const setCurrentCollection = useAPIStore(state=>state.setCurrentCollection)

    return <div>{item?.map((item, i)=>{
            if(isItemGroupDefinition(item)){
                return <Accordion type="single" collapsible style={{marginLeft: `2em`}} key={item.name}>
                    <AccordionItem value="item-1" className="border-none">
                        <AccordionTrigger  onClick={()=>{
                            setCurrentItem({...item,type: DisplayType.SINGLE_TYPE})
                            // @ts-ignore
                            setCurrentCollection({...collection, type: DisplayType.SINGLE_TYPE})
                            console.log("setted",collection)
                        }}><SidebarContextMenu triggerLabel={item.name!} children={<FolderContextMenu collection={item}/>}/></AccordionTrigger>
                        <AccordionContent className="recursive-item border-none">
                            <RecursiveItemGroup key={item.id} item={item.item} collection={collection}/>
                        </AccordionContent>
                    </AccordionItem>
                </Accordion>
            }
            else{
                return <div key={i} style={{marginLeft: `2em`}}
                            onClick={()=>{
                                setCurrentItem({...item,type: DisplayType.SINGLE_TYPE})
                                // @ts-ignore
                                setCurrentCollection({...collection, type: DisplayType.SINGLE_TYPE})
                            }} className="sidebar-request border-none">
                    <APIRequestSidebarIcon type={item.request?.method as string}/>
                    <SidebarContextMenu triggerLabel={item.name as string} children={<ItemContextMenu/>}/>
                </div>
            }
        })}</div>

}


