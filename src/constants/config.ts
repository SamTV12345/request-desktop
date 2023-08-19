import {ItemDefinition} from "postman-collection";

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
