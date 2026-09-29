# Russian (ru). Plural categories: one, few, many, other (CLDR).

## Terms

-brand-name = Cox

## Messages

app-title = { -brand-name }
settings-title = Настройки
quit-app = Завершить { -brand-name }
welcome-user = Добро пожаловать в { -brand-name }, { $name }!

session-count = { $count ->
    [one] { $count } сессия
    [few] { $count } сессии
    [many] { $count } сессий
   *[other] { $count } сессии
}

files-changed = { $name } изменяет { $count ->
    [one] { $count } файл
    [few] { $count } файла
    [many] { $count } файлов
   *[other] { $count } файла
}
