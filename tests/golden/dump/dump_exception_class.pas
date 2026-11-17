program DumpExceptionClass;

// The dumps of an exception class with members (objects.md 15.1, 15.4): the `.type` of the class
// with its field and constructor, `inherited builtin Exception.Create` with the message and
// `builtin ExceptObject` in the hir dump; `ObjSetRef` of `Message` after `inherited Create`, the
// `ExceptObject` intrinsic and `IsClass` of `on` in the ir and bytecode dumps.

type
  ECoded = class(Exception)
  public
    Code: Integer;
    constructor Create(const Msg: string; ACode: Integer);
  end;

constructor ECoded.Create(const Msg: string; ACode: Integer);
begin
  inherited Create(Msg);
  Code := ACode;
end;

begin
  try
    raise ECoded.Create('x', 1);
  except
    on E: ECoded do
      WriteLn(ExceptObject.Message, E.Code);
  end;
end.
