import {Token, TokenLoadResult, TokenWithKey} from "../../models/OAuth2Outcome";
import {parseJWT} from "./TokenManager";
import {useAPIStore} from "../../store/store";
import {MetaData} from "../../models/MetaData";

export const TOKEN_DB = "tokens"
export const ITEM_TO_DB = "item-to-token"
export const META_DB = "meta"


export const TOKEN_DB_VERSION = 2

export const deleteToken = (id: string)=>{
    const dbRq = indexedDB.open(TOKEN_DB, TOKEN_DB_VERSION)
    dbRq.onsuccess = (event)=>{
        const db = dbRq.result
        const transaction = db.transaction(TOKEN_DB, "readwrite")
        const transaction2 = db.transaction(ITEM_TO_DB, "readwrite")
        const objectStore2 = transaction2.objectStore(ITEM_TO_DB)
        const objectStore = transaction.objectStore(TOKEN_DB)
        const request = objectStore.delete(id)

        objectStore2.openCursor().onsuccess = (event:any)=>{
            const cursor = event.target.result
            if (cursor) {
                if(cursor.value === id){
                    objectStore2.delete(cursor.key)
                }
                cursor.continue()
            }
        }
        request.onsuccess = ()=>{
            const filteredTokens = useAPIStore.getState().tokens.filter(token=>token.key !== id)
            useAPIStore.getState().setTokens(filteredTokens)
        }
    }
}

export const insertToken = ( token_key:string, token: string, token_name: string, item_id:string, refresh_token?:string, config?:any)=>{
    const dbRq = indexedDB.open(TOKEN_DB, TOKEN_DB_VERSION)
    dbRq.onsuccess = (event)=>{
        const db = dbRq.result
        const transaction = db.transaction(TOKEN_DB, "readwrite")
        const objectStore = transaction.objectStore(TOKEN_DB)
        const request = objectStore.put({
            access_token: token,
            token_name,
            refresh_token,
            config:config
        }, token_key)
        request.onsuccess = ()=>{
            setTokenToItem(token_key, item_id)
        }
    }
    dbRq.onerror = (event)=>{
        console.log(event)
    }
}


export const setTokenToItem = (token_key:string, item_id:string)=>{
    const dbRq = indexedDB.open(TOKEN_DB, TOKEN_DB_VERSION)
    dbRq.onsuccess = (event)=> {
        const db = dbRq.result
        const transaction = db.transaction(ITEM_TO_DB, "readwrite")
        const objectStore = transaction.objectStore(ITEM_TO_DB)
        objectStore.put(token_key, item_id)
        console.log("Token inserted")
    }
}

export const getToken = (id: string): Promise<Token>=>{
    return new Promise((resolve, reject)=>{
        const dbRq = indexedDB.open(TOKEN_DB, TOKEN_DB_VERSION)
        dbRq.onsuccess = (event)=>{
            const db = dbRq.result
            const transaction = db.transaction(TOKEN_DB, "readwrite")
            const objectStore = transaction.objectStore(TOKEN_DB)
            const request = objectStore.get(id)
            request.onsuccess = ()=>{
                resolve(request.result)
            }
            request.onerror = ()=>{
                reject(request.error)
            }
        }
    })
}

export const getTokenByCollectionId = (postman_id: string): Promise<TokenWithKey>=>{
    return new Promise((resolve, reject)=>{
        const dbRq = indexedDB.open(TOKEN_DB, TOKEN_DB_VERSION)
        dbRq.onsuccess = (event)=>{
            const db = dbRq.result
            const transaction = db.transaction(ITEM_TO_DB, "readwrite")
            const objectStore = transaction.objectStore(ITEM_TO_DB)
            const request:IDBRequest<string> = objectStore.get(postman_id)
            request.onsuccess = ()=> {
                if (!request.result) reject("No token found")
                const transaction = db.transaction(TOKEN_DB, "readwrite")
                const objectStore = transaction.objectStore(TOKEN_DB)
                const request2:IDBRequest<TokenLoadResult> = objectStore.get(request.result)
                request2.onsuccess = ()=>{
                    if (!request2.result) reject("No token found")
                    const token = parseJWT(request2.result)
                    const tokenWithKey = Object.assign(token, {key: request.result})
                    resolve(tokenWithKey)
                }
            }
            request.onerror = ()=>{
                reject(request.error)
            }
        }
    })
}

export const updateToken = (token: Token, key:string)=>{
    const dbRq = indexedDB.open("tokens", TOKEN_DB_VERSION)
    dbRq.onsuccess = (event)=>{
        const db = dbRq.result
        const transaction = db.transaction("tokens", "readwrite")
        const objectStore = transaction.objectStore("tokens")
        const request = objectStore.put(token, key)
        request.onsuccess = ()=>{
            console.log("Token updated")
        }
    }
}

export const getAllTokens = (): Promise<TokenWithKey[]>=>{
    return new Promise(async (resolve, reject)=> {
        const dbRq = indexedDB.open(TOKEN_DB, TOKEN_DB_VERSION)
        const tokens: TokenWithKey[] = []
        dbRq.onsuccess = (event) => {
            const db = dbRq.result
            const transaction = db.transaction(TOKEN_DB, "readwrite")
            const objectStore = transaction.objectStore(TOKEN_DB)
            objectStore.openCursor().onsuccess = (event: any) => {
                const cursor = event.target.result
                if (cursor) {
                    const val = parseJWT(cursor.value)
                    tokens.push({key: cursor.key, ...val})
                    cursor.continue()
                }
                else{
                    resolve(tokens)
                }
            }
            objectStore.openCursor().onerror = (event: any) => {
                reject(event)
            }
        }
    })
}


export const getMetaData = (): Promise<MetaData[]>=>{
    return new Promise(async (resolve, reject)=> {
        const dbRq = indexedDB.open(META_DB, TOKEN_DB_VERSION)
        const tokens: MetaData[] = []
        dbRq.onsuccess = (event) => {
            const db = dbRq.result
            const transaction = db.transaction(META_DB, "readwrite")
            const objectStore = transaction.objectStore(META_DB)
            objectStore.openCursor().onsuccess = (event: any) => {
                const cursor = event.target.result
                if (cursor) {
                    tokens.push(cursor.value)
                    cursor.continue()
                }
                else{
                    resolve(tokens)
                }
            }
            objectStore.openCursor().onerror = (event: any) => {
                reject(event)
            }
        }
        dbRq.onupgradeneeded = (event)=>{
            const db = dbRq.result
            db.createObjectStore(META_DB, {keyPath: "id", autoIncrement: true})
        }
    })
}

export const setMetaData = (meta: MetaData[])=>{
    return new Promise<void>(async (resolve, reject)=> {
    const dbRq = indexedDB.open(META_DB, TOKEN_DB_VERSION)
    dbRq.onsuccess = (event)=>{
        const db = dbRq.result
        const transaction = db.transaction("meta", "readwrite")
        const objectStore = transaction.objectStore("meta")
        objectStore.clear()
        meta.forEach(m=>{
            const request = objectStore.put(m)
            request.onsuccess = ()=>{
                console.log("Meta updated")
            }
        })
        resolve()
    }
    dbRq.onerror = (event)=>{
        reject(event)
    }
        dbRq.onupgradeneeded = (event)=>{
            const db = dbRq.result
            db.createObjectStore(META_DB, {keyPath: "id", autoIncrement: true})
        }
    })
}
