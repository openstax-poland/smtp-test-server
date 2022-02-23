// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import { Mailbox, Group } from '~/src/data'

import GroupOrMailbox from './GroupOrMailbox'

interface Props {
    mailboxes: (Group | Mailbox)[]
}

export default function MailboxList({ mailboxes }: Props) {
    return <div className="mailbox-list">
        {mailboxes.map((mailbox, index) => <>
            {index > 0 && ", "}
            <GroupOrMailbox group={mailbox} />
        </>)}
    </div>
}
