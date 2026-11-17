// ClassName of an instantiated generic class (docs/spec/objects.md chapter 5): the full name
// without the qualification of the definition, with the type arguments as full names. The name
// was cut at the last `.` before, inside the type arguments (`TFoo>`). The messages of a failed
// `as` to a class (2.9) and to an interface (7.6) and the report of an uncaught exception use the
// same name.
program GenericClassName;

uses System.SysUtils;

type
  TFoo = class
  end;

  TBox<T> = class
  public
    type
      TItem = class
      end;
  end;

  EBoxError<T> = class(Exception)
  end;

  TSub = class(TBox<TFoo>)
  end;

  IGetter<T> = interface
    function Get: T;
  end;

  TRefBox<T> = class(TRefObject)
  end;

var
  O: TObject;
  R: TRefBox<TFoo>;
  G: IGetter<TFoo>;
begin
  WriteLn(TBox<Integer>.ClassName);
  WriteLn(TBox<TFoo>.ClassName);
  WriteLn(TBox<TBox<string>>.ClassName);
  WriteLn(TBox<TFoo>.TItem.ClassName);
  WriteLn(TSub.ClassName);
  O := TBox<TFoo>.Create;
  WriteLn(O.ClassName, ' ', O.ToString, ' ', O.ClassNameIs('TBox<GenericClassName.TFoo>'));
  try
    WriteLn((O as TSub).ClassName);
  except
    on E: Exception do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  O.Free;
  R := TRefBox<TFoo>.Create;
  try
    G := R as IGetter<TFoo>;
    WriteLn(G = nil);
  except
    on E: Exception do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    raise EBoxError<TFoo>.Create('boom');
  except
    on E: Exception do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  raise EBoxError<TBox<Integer>>.Create('uncaught');
end.
