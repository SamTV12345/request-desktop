import {Accordion, AccordionContent, AccordionItem, AccordionTrigger} from "../bareComponents/Accordeon";
import {DisplayType, useAPIStore} from "../../store/store";
import {SidebarContextMenu} from "../bareComponents/SidebarContextMenu";
import {FolderContextMenu} from "../collections/FolderContextMenu";
import {ItemGroupDefinition} from "postman-collection";
import {FC} from "react";
import {RecursiveItemGroup} from "../bareComponents/SidebarAccordeon";

type ItemSidebarComponentProps = {
    item: ItemGroupDefinition,
    index: number
}
export const ItemGroupSidebarComponent:FC<ItemSidebarComponentProps> = ({item,index})=>{
    const collection = useAPIStore(state=>state.currentCollection)
    const currentItem = useAPIStore(state=>state.currentItem)
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const setCurrentCollection = useAPIStore(state=>state.setCurrentCollection)

    const openItemGroup = ()=>{
            setCurrentItem({...item,type: DisplayType.SINGLE_TYPE})
            setCurrentCollection({...collection!, type: DisplayType.SINGLE_TYPE})
    }

    const Label = ()=><span onClick={openItemGroup}>{item.name!}</span>

    return <Accordion type="single" collapsible style={{marginLeft: `2em`}} key={item.name}>
        <AccordionItem value="item-1" className={`border-none ${collection?.type === DisplayType.SINGLE_TYPE && item.id === currentItem?.id&& 'bg-background_tertiary'}` }>
            <AccordionTrigger  onClick={openItemGroup}><SidebarContextMenu triggerLabel={<Label/>} children={<FolderContextMenu collection={item}/>}/></AccordionTrigger>
            <AccordionContent className="recursive-item border-none">
                <RecursiveItemGroup key={item.id} items={item.item} collection={collection!}/>
            </AccordionContent>
        </AccordionItem>
    </Accordion>
}
