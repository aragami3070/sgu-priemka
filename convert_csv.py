#!/usr/bin/env python3

import csv
import sys
import tomllib
from pathlib import Path


def load_groups(config_path: Path) -> dict[str, str]:
    with config_path.open("rb") as file:
        config = tomllib.load(file)

    groups = config.get("groups")

    if not isinstance(groups, dict):
        raise ValueError(
            f"В {config_path} отсутствует секция [groups]"
        )

    # В TOML:
    # 151 = "ПИ"
    #
    # А для обработки CSV нам нужно:
    # "ПИ" -> "151"
    return {
        direction.strip(): str(group_number)
        for group_number, direction in groups.items()
    }


def convert_csv(input_path: Path, groups: dict[str, str]) -> Path:
    output_path = input_path.with_name(
        f"{input_path.stem}_new{input_path.suffix}"
    )

    with input_path.open("r", encoding="utf-8-sig", newline="") as src:
        reader = csv.DictReader(src)

        required_columns = {"ФИО", "Почта", "Направление"}

        if reader.fieldnames is None:
            raise ValueError("CSV-файл не содержит заголовка")

        missing = required_columns - set(reader.fieldnames)
        if missing:
            raise ValueError(
                f"В CSV отсутствуют обязательные колонки: {', '.join(missing)}"
            )

        with output_path.open("w", encoding="utf-8", newline="") as dst:
            writer = csv.DictWriter(
                dst,
                fieldnames=[
                    "First",
                    "Last",
                    "Patronymic",
                    "Email",
                    "Group",
                ],
            )

            writer.writeheader()

            for line_number, row in enumerate(reader, start=2):
                # Пропускаем полностью пустые строки:
                # ,,
                # , ,
                # и т.п.
                if not any((value or "").strip() for value in row.values()):
                    continue

                fio = row["ФИО"].strip()
                email = row["Почта"].strip()
                direction = row["Направление"].strip()

                fio_parts = fio.split()

                if len(fio_parts) != 3:
                    raise ValueError(
                        f"Строка {line_number}: ожидалось ФИО из трёх частей, "
                        f"получено: {fio!r}"
                    )

                last, first, patronymic = fio_parts

                group = groups.get(direction)

                if group is None:
                    raise ValueError(
                        f"Строка {line_number}: неизвестное направление "
                        f"{direction!r}"
                    )

                writer.writerow(
                    {
                        "First": first,
                        "Last": last,
                        "Patronymic": patronymic,
                        "Email": email,
                        "Group": group,
                    }
                )

    return output_path


def main() -> None:
    if len(sys.argv) != 2:
        print(f"Использование: {sys.argv[0]} <file.csv>")
        sys.exit(1)

    input_path = Path(sys.argv[1])

    if not input_path.is_file():
        print(f"Файл не найден: {input_path}")
        sys.exit(1)

    # groups.toml ищется рядом с самим скриптом,
    # а не относительно текущей директории терминала.
    config_path = Path(__file__).resolve().parent / "backend/groups.toml"

    if not config_path.is_file():
        print(f"Файл конфигурации не найден: {config_path}")
        sys.exit(1)

    try:
        groups = load_groups(config_path)
        output_path = convert_csv(input_path, groups)
    except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
        print(f"Ошибка: {error}")
        sys.exit(1)

    print(f"Готово: {output_path}")


if __name__ == "__main__":
    main()
