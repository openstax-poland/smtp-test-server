// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import * as React from 'react'

import DateTime from '~/src/components/DateTime'
import MailboxList from '~/src/components/MailboxList'

import { Message } from '~/src/data'

import MessageBody from './components/MessageBody'

import './index.css'

interface Props {
    message: Message
}

export default function MailView({ message }: Props) {
    return <div className="mail-view">
        <div className="details">
            <Field name="From">
                <MailboxList mailboxes={message.from} />
            </Field>
            <Field name="Subject" value={message.subject} />
            <Field name="To">
                <MailboxList mailboxes={message.to} />
            </Field>
            <Field name="Sent">
                <DateTime format="medium" date={new Date(message.date * 1000)} />
            </Field>
        </div>
        <div className="body">
            <MessageBody message={message} />
        </div>
    </div>
}

interface FieldValueProps {
    name: string
    value: string
}

interface FieldChildrenProps {
    name: string
    children: React.ReactNode
}

function Field({ name, ...props }: FieldValueProps | FieldChildrenProps) {
    let children = 'children' in props ? props.children : <span>{props.value}</span>
    return <>
        <span className="field-name">{name}</span>
        {children}
    </>
}
