program ExceptObjectCase;
// `ExceptObject` (objects.md 8.4): `nil` outside every `except` block, also in a `finally` block
// while an exception goes through it; the innermost `except` block being run, from a routine it
// calls too; the outer one again after an inner `try` ends. An exception kept in a `TObject`
// variable comes back with `as Exception` (8.1: an exception is an instance of a class).

uses System.SysUtils;

procedure Show(const Where: string);
begin
  if ExceptObject = nil then
    WriteLn(Where, ': nil')
  else
    WriteLn(Where, ': ', ExceptObject.ClassName, ' ', ExceptObject.Message);
end;

var
  Kept: TObject;
begin
  Show('start');
  try
    try
      raise EConvertError.Create('first');
    finally
      Show('finally');
    end;
  except
    Show('outer');
    try
      raise EInOutError.Create('second');
    except
      Show('inner');
    end;
    Show('outer again');
    Kept := ExceptObject;
  end;
  Show('after');
  WriteLn((Kept as Exception).Message, ' ', Kept is EConvertError);
  Kept := nil;
end.
