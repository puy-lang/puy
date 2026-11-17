program DtorFreeWhileDestroying;

// `Free` / `Destroy` of an instance whose destruction has started (objects.md 3.2): a destructor
// that frees its own instance raises EInvalidOperation instead of destroying it twice. The
// destruction still ends (the parent's destructor is called, 8.5), and the exception reaches the
// `Free` that started it.

uses
  System.SysUtils;

type

  TBase = class
  public
    destructor Destroy; override;
  end;

  TSelfFree = class(TBase)
  public
    UseDestroy: Boolean;
    destructor Destroy; override;
  end;

destructor TBase.Destroy;
begin
  WriteLn('TBase.Destroy');
  inherited;
end;

destructor TSelfFree.Destroy;
begin
  WriteLn('TSelfFree.Destroy');
  if UseDestroy then
    Destroy
  else
    Free;
  WriteLn('not reached');
  inherited;
end;

var
  X: TSelfFree;
  I: Integer;
begin
  for I := 0 to 1 do
  begin
    X := TSelfFree.Create;
    X.UseDestroy := I = 1;
    try
      X.Free;
    except
      on E: Exception do
        WriteLn(E.ClassName, ': ', E.Message);
    end;
  end;
end.
