import {FC, useMemo} from "react";
import {CollectionDefinition, ItemDefinition, ItemGroupDefinition} from "postman-collection";
import {Accordion, AccordionContent, AccordionItem, AccordionTrigger,} from "./Accordeon"
import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "../../store/store";
import {APIRequestSidebarIcon} from "../item/APIRequest";
import {SidebarContextMenu} from "./SidebarContextMenu";
import {CollectionContextMenu} from "../collections/CollectionContextMenu";
import {FolderContextMenu} from "../collections/FolderContextMenu";
import {ItemContextMenu} from "../item/ItemContextMenu";
import {isItemsGroupDefinition} from "../../utils/utils";
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

        return  <Accordion type="single" collapsible  key={collection.info._postman_id+"name"}
                           className={`border-none ${currentCollection?.type === DisplayType.COLLECTION_TYPE
                           && currentCollection.info._postman_id === collection.info._postman_id&& 'bg-background_tertiary'}` }>
            <AccordionItem value="item-1" key={collection.info._postman_id+"item"} className="cursor-pointer border-none">
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


