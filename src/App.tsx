import {useEffect} from "react";
import {invoke} from "@tauri-apps/api/tauri";
import "./App.css";
import {SidebarComponent} from "./sidebar/SidebarComponent";
import {DisplayType, useAPIStore} from "./store/store";
import {Collection} from "postman-collection";
import {ContentModel} from "./components/item/ContentModel";
import {CollectionViewer} from "./components/collections/CollectionViewer";


const ContentModelDecider = ()=>{
    const currentCollection = useAPIStore(state=>state.currentCollection)

    console.log(currentCollection)
    if(currentCollection?.type === DisplayType.COLLECTION_TYPE){
        return <CollectionViewer/>
    }
    else  {
        return <ContentModel/>
    }
}


const App = ()=> {
    const setCollections = useAPIStore(state=>state.setCollections)
    const currentCollection = useAPIStore(state=>state.currentItem)
    async function get_collections():Promise<Collection[]> {
        // Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
        return await invoke("get_collections")
    }

    useEffect(()=>{
        get_collections()
            .then(c=>{
                setCollections(c)
            })
            .catch(e=>console.log(e))
    }, [])

    return (

        <div>
            <div style={{display: "block", float: 'left'}}><SidebarComponent/></div>
            <div className="main-panel"><ContentModelDecider/></div>
        </div>
    )
}

export default App;
