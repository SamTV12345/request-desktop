import {FC, useState} from "react";
import {ItemDefinition, ItemGroupDefinition} from "postman-collection";
import {Accordion, AccordionContent, AccordionItem, AccordionTrigger,} from "./Accordeon"
import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "../../store/store";
import {SidebarContextMenu} from "./SidebarContextMenu";
import {CollectionContextMenu} from "../collections/CollectionContextMenu";
import {ItemSidebarComponent} from "../item/ItemSidebarComponent";
import {ItemGroupSidebarComponent} from "../item/ItemGroupSidebarComponent";


type SidebarAccordeonProps = {
    collection: CollectionDefinitionExtended
}

export const isItemGroupDefinition = (item: any): item is ItemGroupDefinition => {
    return 'item' in item && item.item != undefined
}

export const SidebarAccordeon:FC<SidebarAccordeonProps> = ({collection}) => {
    const setCurrentCollection = useAPIStore(state=>state.setCurrentCollection)
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const currentCollection = useAPIStore(state=>state.currentCollection)
    const metadata = useAPIStore(state=>state.metadata)
    const isOpen = ()=>{
        if(metadata.has(collection.info._postman_id)){
            return metadata.get(collection.info._postman_id)?.open? collection.info._postman_id: undefined
        }
        else{
            return ""
        }
    }

    const [open,setOpen] = useState<string|undefined>(()=>isOpen())

    const changeOpen = (v: string)=>{
        if (v.length>0){
            metadata.set(v, {open: true, id: collection.info._postman_id})
            setOpen(v)
        }
        else{
            metadata.set(collection.info._postman_id, {open: false, id: collection.info._postman_id})
            setOpen(v)
        }
    }

        return  <Accordion type="single"  collapsible key={collection.info._postman_id+"name"} value={open} onValueChange={c=>changeOpen(c)}
                           className={`border-none ${currentCollection?.type === DisplayType.COLLECTION_TYPE
                           && currentCollection.info._postman_id === collection.info._postman_id&& 'bg-background_tertiary'}` }>
            <AccordionItem value={collection.info._postman_id} key={collection.info._postman_id+"item"} className="cursor-pointer border-none">
                    <AccordionTrigger><span onClick={()=>{
                        setCurrentCollection({...collection, type: DisplayType.COLLECTION_TYPE})
                        setCurrentItem(undefined)
                    }}><SidebarContextMenu triggerLabel={collection.info?.name as string} children={<CollectionContextMenu collection={collection}/>}/></span></AccordionTrigger>
                        <AccordionContent className="">
                            <RecursiveItemGroup items={collection.item} collection={collection}/>
                        </AccordionContent>
            </AccordionItem>
    </Accordion>
}

type RecursiveItemGroupProps = {
    items: (ItemGroupDefinition | ItemDefinition)[] | undefined,
    collection: CollectionDefinitionExtended
}
export const RecursiveItemGroup:FC<RecursiveItemGroupProps> = ({items, collection})=>{

    return <div>{items?.map((item, i)=>{
            if(isItemGroupDefinition(item)){
                return <ItemGroupSidebarComponent index={i} item={item} key={item.id} collection={collection}/>
            }
            else{
                return <ItemSidebarComponent index={i} item={item} key={item.id} collection={collection}/>
            }
        })}</div>

}


