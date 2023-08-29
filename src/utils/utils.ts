import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"
import {ItemGroupDefinition, QueryParamDefinition} from "postman-collection";
import {TokenWithKey} from "../models/OAuth2Outcome";

export function cn(...inputs: ClassValue[]) {
    return twMerge(clsx(inputs))
}

export const convertSearchParamsToString = (searchParams: string[]) => {

}

export const copyToClipboard = (text: string) => {
    return navigator.clipboard.writeText(text)
}

export const isItemsGroupDefinition = (item: any): item is ItemGroupDefinition => {
    return item &&item.items !== undefined && item.auth !== undefined
}

export const isTokenExpired = (token:TokenWithKey)=>{
    return token.exp*1000 < Date.now()
}

export const isQueryParamList = (param: any): param is QueryParamDefinition[] => {
    return param && Array.isArray(param)
}
