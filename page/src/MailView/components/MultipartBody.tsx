// Copyright 2022 OpenStax Poland
// Licensed under the MIT license. See LICENSE file in the project root for
// full license text.

import { Message, Multipart } from '~/src/data'

import MultipartAlternative from './MultipartAlternative'

export interface Props {
    message: Message
    part?: string
    data: Multipart
}

export default function MultipartBody({ message, part, data }: Props) {
    switch (data.kind) {
    case 'mixed':
        return <div>multipart/mixed is not supported</div>

    case 'alternative':
        return <MultipartAlternative message={message} part={part} data={data} />
    }
}
