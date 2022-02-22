import * as React from 'react'

interface Props {
    format?: 'tiny' | 'short' | 'medium'
    date: Date
}

export default function DateTime({ format = 'short', date }: Props) {
    let dateStyle: Intl.DateTimeFormatOptions['dateStyle']
    let timeStyle: Intl.DateTimeFormatOptions['timeStyle'] = 'short'

    switch (format) {
    case 'tiny':
        if (!isNow(date)) {
            dateStyle = 'short'
        }
        break

    case 'short':
        dateStyle = 'short'
        break

    case 'medium':
        dateStyle = 'medium'
        timeStyle = 'medium'
        break
    }

    const intl = new Intl.DateTimeFormat(navigator.language, { dateStyle, timeStyle })

    return intl.format(date)
}

function isNow(date: Date) {
    const now = new Date(Date.now())
    return now.getUTCFullYear() == date.getUTCFullYear()
        && now.getUTCMonth() == date.getUTCMonth()
        && now.getUTCDay() == date.getUTCDay()
}
