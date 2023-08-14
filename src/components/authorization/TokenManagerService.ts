import {Token} from "../../models/OAuth2Outcome";

export const deleteToken = (id: string)=>{
    const dbRq = indexedDB.open("tokens", 1)
    dbRq.onsuccess = (event)=>{
        const db = dbRq.result
        const transaction = db.transaction("tokens", "readwrite")
        const objectStore = transaction.objectStore("tokens")
        const request = objectStore.delete(id)
        request.onsuccess = ()=>{
            console.log("Token deleted")
        }
    }
}


export const insertToken = ( key:string, token: string,token_name:string)=>{
    const dbRq = indexedDB.open("tokens", 1)
    dbRq.onsuccess = (event)=>{
        const db = dbRq.result
        const transaction = db.transaction("tokens", "readwrite")
        const objectStore = transaction.objectStore("tokens")
        const request = objectStore.add({
            token,
            token_name
        }, key)
        request.onsuccess = ()=>{
            console.log("Token inserted")
        }
    }
}

export const getToken = (id: string): Promise<Token>=>{
    return new Promise((resolve, reject)=>{
        const dbRq = indexedDB.open("tokens", 1)
        dbRq.onsuccess = (event)=>{
            const db = dbRq.result
            const transaction = db.transaction("tokens", "readwrite")
            const objectStore = transaction.objectStore("tokens")
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

export const updateToken = (token: Token, key:string)=>{
    const dbRq = indexedDB.open("tokens", 1)
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
