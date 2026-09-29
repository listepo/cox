# Ukrainian (uk). Plural categories: one, few, many, other (CLDR).

## Terms

-brand-name = Cox

## Messages

app-title = { -brand-name }
settings-title = Налаштування
quit-app = Вийти з { -brand-name }
welcome-user = Ласкаво просимо до { -brand-name }, { $name }!

session-count = { $count ->
    [one] { $count } сесія
    [few] { $count } сесії
    [many] { $count } сесій
   *[other] { $count } сесії
}

files-changed = { $name } змінює { $count ->
    [one] { $count } файл
    [few] { $count } файли
    [many] { $count } файлів
   *[other] { $count } файлу
}
