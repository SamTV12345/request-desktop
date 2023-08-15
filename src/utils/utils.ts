import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
    return twMerge(clsx(inputs))
}

export const convertSearchParamsToString = (searchParams: string[]) => {

}

export const copyToClipboard = (text: string) => {
    return navigator.clipboard.writeText(text)
}
