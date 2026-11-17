program ExceptionToString;
// `ToString` of an exception returns `Message` (objects.md 8.2: the built-in body 2 of the
// table), through the table (a variable of type `TObject`), statically (`inherited ToString` in an
// override), and on `ExceptObject` in an `except` block. Delphi does the same; FPC 3.2.2 adds the
// class name, so this is not a diff case.

uses System.SysUtils;

type
  EMine = class(Exception)
  public
    function ToString: string; override;
  end;

  EPlain = class(Exception);

function EMine.ToString: string;
begin
  Result := 'EMine[' + inherited ToString + ']';
end;

var
  O: TObject;
  E: Exception;
begin
  E := Exception.Create('base');
  WriteLn(E.ToString);
  O := EPlain.Create('plain');
  WriteLn(O.ToString);
  O := EMine.Create('mine');
  WriteLn(O.ToString);
  E := EConvertError.CreateFmt('%d is wrong', [7]);
  WriteLn(E.ToString);
  try
    raise EMine.Create('raised');
  except
    WriteLn(ExceptObject.ToString);
  end;
end.
