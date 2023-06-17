import {useEffect} from "react";
import { invoke } from "@tauri-apps/api/tauri";
import "./App.css";
import {SidebarComponent} from "./sidebar/SidebarComponent";
import {useAPIStore} from "./store/store";
import {Collection} from "postman-collection";
import {ContentModel} from "./components/ContentModel";

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

        <div style={{display: 'flex'}}>
            <div style={{display: "block", overflow: 'hidden'}}><SidebarComponent/></div>
            {currentCollection&&currentCollection.name?
            <ContentModel/>:<div>Loading...</div>}
        </div>
    )
}

export default App;
