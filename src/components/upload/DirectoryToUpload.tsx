import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import {useDropzone} from "react-dropzone";
import React, {useEffect, useState} from "react";
import {open } from "@tauri-apps/api/dialog";
import {invoke} from "@tauri-apps/api/tauri";
import {CollectionDefinition} from "postman-collection";
import {VERSIONS} from "../../constants";
import {FileUpload} from "../../models/FileUpload";

export const DirectoryToUpload = ()=>{
    const [uploadedCollections,setUploadedCollections] = useState<CollectionDefinitionExtended[]>()
    const collections = useAPIStore(state=>state.collections)
    const setCollectionsStore = useAPIStore(state=>state.setCollections)

    const DirectoryToUpload = ()=>{
        const addCollection = useAPIStore(state=>state.addCollection)

        const addFolders = (collection:CollectionDefinitionExtended)=>{
            invoke<CollectionDefinitionExtended>("insert_collection", {collection: collection})
                .then((c)=>{
                    setCollectionsStore([...collections, c])
                })
                .catch(e=>console.log(e))
        }

        return  <>
            <h2 className="import-heading text-2xl">The following file will be uploaded</h2>
            <div>
            <table className="import-table">
                <thead>
                <tr>
                    <th scope="col">Name</th>
                    <th scope="col">Format</th>
                    <th scope="col">Import as</th>
                </tr>
                </thead>
                <tbody>
                {uploadedCollections?.map(c=> <tr>
                    <td>{c.info?.name}</td>
                    {/* // @ts-ignore*/}
                    <td>{VERSIONS.has(c.info?.schema!)&&VERSIONS.get(c.info?.schema!)}</td>
                    <td>Collection</td>
                </tr>)}
                </tbody>
            </table>
            <div className="button-bar">
                <button className="upload-cancel-button" >Cancel</button>
                <button className="upload-preview-button" onClick={()=>{
                    uploadedCollections?.forEach(c=>{
                        addFolders(c)
                    })
                }}>Import</button>
            </div>
        </div>
            </>

    }


    const importFolder = ()=> {
        open({directory: true, multiple:false}).then((result) => {
           invoke<CollectionDefinitionExtended[]>("get_postman_files_from_dir",{
               path: result
           })
               .then(c=>setUploadedCollections(c))
        })
    }

    return (
        uploadedCollections?<DirectoryToUpload/>:<div className="dropzone">
            <button className="bg-gray-500 p-2 rounded hover:bg-gray-400" onClick={importFolder}>Click here to add your folder</button>
        </div>
    )
}
