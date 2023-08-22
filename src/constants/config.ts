import {ItemDefinition, ItemGroupDefinition} from "postman-collection";

export const getDefaultRequest:()=>ItemDefinition = ()=> {
    return {
        request: {
            url: "",
            method: "GET",
            header: [],
        },
        name: 'New Request',
        id: crypto.randomUUID(),
        disabled: false,
        response: [],
        events: [],
        description: ''
    }
}

export const getDefaultFolder:()=>ItemGroupDefinition = ()=> {
    return {
        auth: undefined,
        description: '',
        event: [],
        id: crypto.randomUUID(),
        item: [],
        name: 'New Folder',
    } satisfies ItemGroupDefinition
}
