import React, {useState} from "react";
import {CollectionDefinitionExtended, useAPIStore} from "../../store/store";
import {invoke} from "@tauri-apps/api/tauri";
import {VERSIONS} from "../../constants";
// @ts-ignore
import converter from 'openapi2postman-unofficial'

export const ImportByLink = () => {
    const [importLink, setImportLink] = useState<string>("")
    const [postmanCollection, setPostmanCollection] = useState<CollectionDefinitionExtended>()


    const importCollectionFromURL = async (url: string) => {
        invoke<String>('download_from_url', {
            url
        })
            .then(c => {
                converter.convert({type: 'string', data: JSON.stringify(c)}, {}, (err:any, conversionResult:any) => {
                    if (!conversionResult.result) {
                        console.log('Could not convert', conversionResult.reason);
                    }
                    else {
                        setPostmanCollection(conversionResult.output[0].data)
                    }
                })
            })
    }


    const ImportPreview = () => {
        const setCollectionsStore = useAPIStore(state => state.setCollections)
        const collections = useAPIStore(state => state.collections)
        const addFolders = (collection: CollectionDefinitionExtended) => {
            invoke<CollectionDefinitionExtended>("insert_collection", {collection: collection})
                .then((c) => {
                    setCollectionsStore([...collections, c])
                })
                .catch(e => console.log(e))
        }

        return <>
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
                    {postmanCollection && <tr>
                        <td>{postmanCollection.info?.name}</td>
                        {/* // @ts-ignore*/}
                        <td>{VERSIONS.has(postmanCollection.info?.schema!) && VERSIONS.get(postmanCollection.info?.schema!)}</td>
                        <td>Collection</td>
                    </tr>}
                    </tbody>
                </table>
                {postmanCollection && <div className="button-bar">
                    <button className="upload-cancel-button">Cancel</button>
                    <button className="upload-preview-button" onClick={() => {
                        addFolders(postmanCollection)
                    }}>Import
                    </button>
                </div>}
            </div>
        </>
    }
    return <>
        {postmanCollection ? <ImportPreview/> : <>
            <h2 className="text-white">Enter a URL</h2><input type="text"
                                                              className="border-[2px] border-border_strong bg-background_tertiary hover:bg- p-1 rounded text-white w-full placeholder-gray-500"
                                                              placeholder="e.g. https://petstore.swagger.io/v2/swagger.json"
                                                              value={importLink}
                                                              onChange={v => setImportLink(v.target.value)}/><br/>
            <button className="bg-mustard-600 pl-2 pr-2 pt-1 pb-1 rounded mt-2 text-white"
                    onClick={() => importCollectionFromURL(importLink)}>Continue
            </button>
        </>
        }
    </>
}
