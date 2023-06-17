import {FC} from "react";
import {CollectionDefinition, ItemDefinition, ItemGroupDefinition} from "postman-collection";
import {types} from "util";
import {PagesRounded, PagesTwoTone} from "@mui/icons-material";
import {
    Accordion,
    AccordionContent,
    AccordionItem,
    AccordionTrigger,
} from "../components/Accordeon"
import {useAPIStore} from "../store/store";


type SidebarAccordeonProps = {
    collection: CollectionDefinition
}

export const isItemGroupDefinition = (item: any): item is ItemGroupDefinition => {
    return 'item' in item && item.item != undefined
}

export const SidebarAccordeon:FC<SidebarAccordeonProps> = ({collection}) => {
    const initialNumber = 0
    return <div>
        {collection.info?.name}
        <RecursiveItemGroup item={collection.item} indent={initialNumber}/>
    </div>
}

type RecursiveItemGroupProps = {
    item: (ItemGroupDefinition | ItemDefinition)[] | undefined,
    indent: number
}
export const RecursiveItemGroup:FC<RecursiveItemGroupProps> = ({item,indent})=>{
    const setCurrentItem = useAPIStore(state=>state.setCurrentItem)

    return <div>{item?.map((item, i)=>{
            if(isItemGroupDefinition(item)){
                return <Accordion type="single" collapsible style={{margin: `${indent*10}%`}} key={item.name}>
                    <AccordionItem value="item-1">
                        <AccordionTrigger  onClick={()=>{setCurrentItem(item)}}>{item.name}</AccordionTrigger>
                        <AccordionContent>
                            <RecursiveItemGroup item={item.item} indent={indent+1}/>
                        </AccordionContent>
                    </AccordionItem>
                </Accordion>
            }
            else{
                return <div key={i} onClick={()=>{setCurrentItem(item)}}>
                    <PagesTwoTone/>
                    {item.name}
                </div>
            }
        })}</div>

}
