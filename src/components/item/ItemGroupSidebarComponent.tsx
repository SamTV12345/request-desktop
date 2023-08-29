import {Accordion, AccordionContent, AccordionItem, AccordionTrigger} from "../bareComponents/Accordeon";
import {CollectionDefinitionExtended, DisplayType, useAPIStore} from "../../store/store";
import {SidebarContextMenu} from "../bareComponents/SidebarContextMenu";
import {FolderContextMenu} from "../collections/FolderContextMenu";
import {ItemGroupDefinition} from "postman-collection";
import {FC, useState} from "react";
import {RecursiveItemGroup} from "../bareComponents/SidebarAccordeon";
import {setMetaData} from "../collection_authorization/TokenManagerService";
import {useNavigate} from "react-router-dom";

type ItemSidebarComponentProps = {
    item: ItemGroupDefinition,
    index: number,
    collection:CollectionDefinitionExtended
}
export const ItemGroupSidebarComponent:FC<ItemSidebarComponentProps> = ({item,index, collection})=>{
    const currentItem = useAPIStore(state=>state.currentItem)
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)
    const setCurrentCollection = useAPIStore(state=>state.setCurrentCollection)
    const navigate = useNavigate()
    const metadata = useAPIStore(state=>state.metadata)
    const isOpen = ()=>{
        if(metadata.has(item!.id!)){
            return metadata.get(item!.id!)?.open? item!.id!: undefined
        }
        else{
            return ""
        }
    }

    const [open,setOpen] = useState<string|undefined>(()=>isOpen())

    const changeOpen = (v: string)=>{
        if (v.length>0){
            metadata.set(v, {open: true, id: item!.id!})
            setOpen(v)
        }
        else{
            metadata.set(item!.id!, {open: false, id:item!.id!})
            setOpen(v)
        }
    }
    const openItemGroup = ()=>{
            // Unselect old one
            if(currentItem) {
                const valInMap = metadata.get(currentItem.id!)
                if(valInMap){
                    metadata.set(currentItem.id!, {...valInMap, selected: false, id: item.id!})
                }
            }
            setCurrentItem({...item,type: DisplayType.SINGLE_TYPE})
            setCurrentCollection({...collection!, type: DisplayType.SINGLE_TYPE})
            const valInMap = metadata.get(item.id!)
            if(valInMap){
                metadata.set(item.id!, {...valInMap, selected: false, id: item.id!})
            }
        navigate("/group")
    }

    const Label = ()=><span onClick={openItemGroup}>{item.name!}</span>

    return <Accordion type="single" collapsible style={{marginLeft: `2em`}} key={item.name} value={open} onValueChange={(v)=>changeOpen(v)}>
        <AccordionItem value={item.id!} className={`border-none ${collection?.type === DisplayType.SINGLE_TYPE && item.id === currentItem?.id&& 'bg-background_tertiary'}` }>
            <AccordionTrigger  onClick={openItemGroup}><SidebarContextMenu triggerLabel={<Label/>} children={<FolderContextMenu collection={collection} item={item}/>}/></AccordionTrigger>
            <AccordionContent className="recursive-item border-none">
                <RecursiveItemGroup key={item.id} items={item.item} collection={collection!}/>
            </AccordionContent>
        </AccordionItem>
    </Accordion>
}
