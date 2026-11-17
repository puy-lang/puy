program HintErrors;

// Hint directives where they cannot be written (attributes.md 6.1): on the forward declaration of
// a class or an interface (E3401, pointing at the full declaration), and on the body of a forward
// routine or a method without being on its declaration (E3401; the same directive written again
// is accepted). The same directive twice in one declaration is E0401.

type
  TFwd = class deprecated;
  IFwd = interface experimental 'soon';
  TFwd = class
    procedure Run; deprecated;
    procedure Go;
  end;
  IFwd = interface
  end;
  TTwice = Integer deprecated deprecated 'again';

procedure Later; forward; deprecated;

procedure Later; deprecated; experimental;
begin
end;

procedure Plain; forward;

procedure Plain; deprecated;
begin
end;

procedure TFwd.Run; deprecated;
begin
end;

procedure TFwd.Go; experimental;
begin
end;

procedure Dup; deprecated; experimental; deprecated 'x';
begin
end;

begin
end.
