import * as Dialog from '@radix-ui/react-dialog';
import {NavBox} from "../components/routing/NavBox";
import {FolderIcon, ZapIcon, HistoryIcon, Monitor, Container} from "lucide-react";


export const RootSidebar = ()=>{
    return <div className="">
        <div className="root-dialog border-r-[1px] border-white flex gap-5 flex-col text-white p-4 h-screen">
            <NavBox title="Collections" to="collection" logo={<FolderIcon/>}/>
            <NavBox title="APIs" to="apis" logo={<ZapIcon/>}/>
            <NavBox title="Environments" to="environments" logo={<Container/>}/>
            <NavBox title="Monitors" to="monitors"  logo={<Monitor/>}/>
            <NavBox title="History" to="history" logo={<HistoryIcon/>}/>
        </div>
    </div>
}