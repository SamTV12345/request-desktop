import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"
import {ItemGroupDefinition} from "postman-collection";

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
